"""Converts the DAT2 cover upscaler to a compiled Core ML model.

Run through scripts/build-upscale-model.sh, which pins the inputs. The tile
size is fixed in the model: crates/upscale's `TILE` must match.
"""

import sys

import coremltools as ct
import coremltools.converters.mil.frontend.torch.ops as ct_ops
import numpy as np
import torch
from spandrel import ModelLoader

TILE = 288

# coremltools' int() cast rejects one-element arrays, which DAT's window
# arithmetic produces while tracing. Unwrap them first.
_cast = ct_ops._cast


def cast_unwrapping_one_element_arrays(context, node, dtype, dtype_name):
    value = ct_ops._get_inputs(context, node, expected=1)[0].val
    if value is not None and getattr(value, "size", 0) == 1 and value.ndim > 0:
        context.add(ct_ops.mb.const(val=dtype(value.item()), name=node.name))
        return
    _cast(context, node, dtype, dtype_name)


ct_ops._cast = cast_unwrapping_one_element_arrays


def main(weights: str, package: str, compiled: str) -> None:
    model = ModelLoader(device="cpu").load_from_file(weights).model.eval()
    example = torch.rand(1, 3, TILE, TILE)
    with torch.no_grad():
        traced = torch.jit.trace(model, example, check_trace=False)
    converted = ct.convert(
        traced,
        inputs=[ct.TensorType(name="input", shape=example.shape, dtype=np.float32)],
        outputs=[ct.TensorType(name="output", dtype=np.float32)],
        convert_to="mlprogram",
        minimum_deployment_target=ct.target.macOS15,
        compute_precision=ct.precision.FLOAT16,
    )
    converted.short_description = "4xBHI_dat2_real by Philip Hofmann, CC BY 4.0"
    converted.save(package)
    ct.utils.compile_model(package, compiled)


if __name__ == "__main__":
    main(*sys.argv[1:4])
