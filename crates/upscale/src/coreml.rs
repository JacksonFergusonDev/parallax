//! The only code that touches Core ML, and the crate's only unsafe code.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::path::Path;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel,
    MLModelConfiguration, MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSInteger, NSNumber, NSString, NSURL};

use crate::{Engine, Error, Result, SCALE, TILE};

/// A loaded model. Slow to load, so keep it around between covers.
pub struct CoreMl {
    model: Retained<MLModel>,
    input_name: Retained<NSString>,
    output_name: Retained<NSString>,
}

// SAFETY: Core ML models may be used from any thread, and `&mut`-free access
// from one thread at a time is all `Send` promises here.
unsafe impl Send for CoreMl {}

fn model_error(error: Retained<NSError>) -> Error {
    Error::Model(error.localizedDescription().to_string())
}

impl CoreMl {
    /// Loads a compiled `.mlmodelc` folder.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_dir() {
            return Err(Error::Unavailable(format!(
                "no model at {}",
                path.display()
            )));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        // SAFETY: plain Objective-C calls with valid, retained arguments.
        let model = unsafe {
            let configuration = MLModelConfiguration::new();
            // The Neural Engine never finishes loading this model.
            configuration.setComputeUnits(MLComputeUnits::CPUAndGPU);
            MLModel::modelWithContentsOfURL_configuration_error(&url, &configuration)
        }
        .map_err(model_error)?;
        Ok(Self {
            model,
            input_name: NSString::from_str("input"),
            output_name: NSString::from_str("output"),
        })
    }

    fn predict(&self, tile: &[f32]) -> Result<Vec<f32>> {
        let side = TILE as isize;
        let shape: Vec<_> = [1, 3, side, side]
            .into_iter()
            .map(NSNumber::new_isize)
            .collect();
        let shape = NSArray::from_retained_slice(&shape);
        // SAFETY: arguments are valid and the buffer handed to the block is
        // only written within the size Core ML reports for it.
        unsafe {
            let input = MLMultiArray::initWithShape_dataType_error(
                MLMultiArray::alloc(),
                &shape,
                MLMultiArrayDataType::Float32,
            )
            .map_err(model_error)?;
            let write = RcBlock::new(
                |bytes: NonNull<std::ffi::c_void>,
                 size: NSInteger,
                 _: NonNull<NSArray<NSNumber>>| {
                    let count = tile.len().min(size as usize / size_of::<f32>());
                    std::ptr::copy_nonoverlapping(tile.as_ptr(), bytes.as_ptr().cast(), count);
                },
            );
            input.getMutableBytesWithHandler(&write);

            let value = MLFeatureValue::featureValueWithMultiArray(&input);
            let features = NSDictionary::<NSString, AnyObject>::from_slices(
                &[&*self.input_name],
                &[value.as_ref()],
            );
            let provider = MLDictionaryFeatureProvider::initWithDictionary_error(
                MLDictionaryFeatureProvider::alloc(),
                &features,
            )
            .map_err(model_error)?;
            let result = self
                .model
                .predictionFromFeatures_error(ProtocolObject::from_ref(&*provider))
                .map_err(model_error)?;
            read_output(&result, &self.output_name)
        }
    }
}

/// Copies the output into a dense planar buffer. Core ML may pad rows, so
/// this follows the strides it reports rather than assuming a layout.
///
/// # Safety
/// `result` must be a prediction from this model.
unsafe fn read_output(
    result: &ProtocolObject<dyn MLFeatureProvider>,
    name: &NSString,
) -> Result<Vec<f32>> {
    let missing = || Error::Model("the model returned no image".into());
    // SAFETY: plain Objective-C calls; reads stay within the bounds checked
    // against the size Core ML reports for the buffer.
    unsafe {
        let output = result
            .featureValueForName(name)
            .and_then(|value| value.multiArrayValue())
            .ok_or_else(missing)?;
        if output.dataType() != MLMultiArrayDataType::Float32 {
            return Err(Error::Model("unexpected output type".into()));
        }
        let side = (TILE * SCALE) as usize;
        let strides: Vec<usize> = output
            .strides()
            .iter()
            .map(|stride| stride.as_isize() as usize)
            .collect();
        let shape: Vec<usize> = output
            .shape()
            .iter()
            .map(|dimension| dimension.as_isize() as usize)
            .collect();
        if shape != [1, 3, side, side] {
            return Err(Error::Model(format!("unexpected output shape {shape:?}")));
        }
        let dense = RefCell::new(Err(missing()));
        let read = RcBlock::new(|bytes: NonNull<std::ffi::c_void>, size: NSInteger| {
            let values = size as usize / size_of::<f32>();
            let last = 2 * strides[1] + (side - 1) * (strides[2] + strides[3]);
            *dense.borrow_mut() = if last < values {
                let source = std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), values);
                let mut out = Vec::with_capacity(3 * side * side);
                for channel in 0..3 {
                    for y in 0..side {
                        let row = channel * strides[1] + y * strides[2];
                        out.extend((0..side).map(|x| source[row + x * strides[3]]));
                    }
                }
                Ok(out)
            } else {
                Err(Error::Model("output smaller than its shape".into()))
            };
        });
        output.getBytesWithHandler(&read);
        drop(read);
        dense.into_inner()
    }
}

impl Engine for CoreMl {
    fn run(&self, tile: &[f32]) -> Result<Vec<f32>> {
        // Each prediction leaves autoreleased buffers behind; worker threads
        // have no run loop to drain them.
        autoreleasepool(|_| self.predict(tile))
    }
}
