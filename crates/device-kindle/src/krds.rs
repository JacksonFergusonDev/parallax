//! Kindle reader data store (KRDS) sidecars in a book's `.sdr` folder:
//! `.azw3f`/`.yjf`/`.mbs` hold reading position and time, and
//! `.azw3r`/`.yjr`/`.mbp1` hold annotations and settings. Ported from John
//! Howell's `krds.py` (GPL v3, <https://github.com/K-R-D-S/KRDS>), with the
//! tolerance for newer firmware fields from zevisvei/kindle-reading-dashboard.
//!
//! The format is undocumented and changes between firmware releases, so
//! anything not understood is skipped rather than failing the whole file.

use std::collections::BTreeMap;

const SIGNATURE: &[u8] = b"\x00\x00\x00\x00\x00\x1A\xB1\x26";

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// A named structure and its values in order.
    Object(String, Vec<Value>),
}

impl Value {
    fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let bytes = self.data.get(self.at..self.at + n)?;
        self.at += n;
        Some(bytes)
    }

    fn i8(&mut self) -> Option<i8> {
        self.take(1).map(|b| i8::from_be_bytes([b[0]]))
    }

    fn peek_i8(&self) -> Option<i8> {
        self.data.get(self.at).map(|b| i8::from_be_bytes([*b]))
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn utf(&mut self) -> Option<String> {
        if self.bool()? {
            return Some(String::new());
        }
        let len = u16::from_be_bytes(self.array()?);
        Some(String::from_utf8_lossy(self.take(usize::from(len))?).into_owned())
    }

    fn bool(&mut self) -> Option<bool> {
        match self.i8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn value(&mut self) -> Option<Value> {
        let kind = self.i8()?;
        Some(match kind {
            0 => Value::Bool(self.bool()?),
            1 => Value::Int(i64::from(i32::from_be_bytes(self.array()?))),
            2 => Value::Int(i64::from_be_bytes(self.array()?)),
            3 => Value::Text(self.utf()?),
            4 => Value::Float(f64::from_be_bytes(self.array()?)),
            5 => Value::Int(i64::from(i16::from_be_bytes(self.array()?))),
            6 => Value::Float(f64::from(f32::from_be_bytes(self.array()?))),
            7 => Value::Int(i64::from(self.i8()?)),
            9 => Value::Text(char::from(self.take(1)?[0]).to_string()),
            -2 => {
                let name = self.utf()?;
                let mut values = Vec::new();
                while self.peek_i8()? != -1 {
                    values.push(self.value()?);
                }
                self.i8()?;
                Value::Object(name, values)
            }
            _ => return None,
        })
    }
}

/// The top-level structures in a KRDS file, by name. `None` if it isn't one
/// or it's cut short.
pub fn parse(data: &[u8]) -> Option<BTreeMap<String, Vec<Value>>> {
    if !data.starts_with(SIGNATURE) {
        return None;
    }
    let mut r = Reader {
        data,
        at: SIGNATURE.len(),
    };
    if r.value()?.as_i64()? != 1 {
        return None;
    }
    let count = r.value()?.as_i64()?;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        if let Value::Object(name, values) = r.value()? {
            out.insert(name, values);
        }
    }
    Some(out)
}

/// A position and when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// A byte offset into the book's text for MOBI and KF8 books; other
    /// forms (KFX) are kept as text.
    pub position: String,
    /// Milliseconds since the Unix epoch.
    pub time_ms: Option<i64>,
}

impl Position {
    pub fn offset(&self) -> Option<u64> {
        self.position.trim().parse().ok()
    }
}

fn position(values: &[Value], name: &str) -> Option<Position> {
    // krds.py `_decode_known`: lpr is a version then position and time (or a
    // bare position in the oldest form); fpr and updated_lpr are position
    // then time.
    let (pos, time) = match (name, values.first()?) {
        ("lpr", Value::Text(p)) => (p.clone(), None),
        ("lpr", Value::Int(v)) if *v <= 2 => (
            values.get(1)?.as_text()?.to_owned(),
            values.get(2).and_then(Value::as_i64),
        ),
        ("lpr", _) => return None,
        (_, Value::Text(p)) => (p.clone(), values.get(1).and_then(Value::as_i64)),
        _ => return None,
    };
    Some(Position {
        position: pos,
        time_ms: time.filter(|t| *t > 0),
    })
}

/// What a book's sidecars say about reading it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadingData {
    /// Last page read.
    pub last: Option<Position>,
    /// Furthest page read.
    pub furthest: Option<Position>,
    /// Total time spent reading, in milliseconds (`timer.model`).
    pub total_time_ms: Option<i64>,
    /// Share of the book read, 0 to 1, counting re-reading (`timer.model`).
    pub total_percent: Option<f64>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// "bookmark", "highlight", "note" or another Kindle class name.
    pub kind: String,
    pub start: String,
    pub end: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: Option<i64>,
    pub note: Option<String>,
}

impl Annotation {
    /// A stable id for the annotation, from its kind and range, so syncing
    /// again updates it instead of adding a copy.
    pub fn id(&self) -> String {
        let key = format!("{}\n{}\n{}", self.kind, self.start, self.end);
        format!(
            "krds-{}",
            &sha1_smol::Sha1::from(key).digest().to_string()[..16]
        )
    }
}

const ANNOTATION_KINDS: &[(i64, &str)] = &[
    (0, "bookmark"),
    (1, "highlight"),
    (2, "note"),
    (3, "clip_article"),
    (10, "handwritten_note"),
    (11, "sticky_note"),
    (13, "underline"),
];

fn annotations(values: &[Value]) -> Vec<Annotation> {
    // A count, then for each kind its code and a `saved.avl.interval.tree`
    // holding a count and that many `annotation.personal.*` objects.
    let mut out = Vec::new();
    let mut it = values.iter().skip(1);
    while let (Some(code), Some(Value::Object(_, tree))) = (it.next(), it.next()) {
        let kind = code
            .as_i64()
            .and_then(|c| ANNOTATION_KINDS.iter().find(|(k, _)| *k == c))
            .map_or("annotation", |(_, name)| name);
        for item in tree.iter().skip(1) {
            let Value::Object(_, fields) = item else {
                continue;
            };
            let (Some(start), Some(end)) = (
                fields.first().and_then(Value::as_text),
                fields.get(1).and_then(Value::as_text),
            ) else {
                continue;
            };
            out.push(Annotation {
                kind: kind.to_owned(),
                start: start.to_owned(),
                end: end.to_owned(),
                created_ms: fields.get(2).and_then(Value::as_i64),
                note: (kind == "note")
                    .then(|| fields.get(5).and_then(Value::as_text).map(str::to_owned))
                    .flatten()
                    .filter(|n| !n.is_empty()),
            });
        }
    }
    out
}

/// Merges what the sidecar files of one book say. Pass every KRDS file in
/// the `.sdr` folder; unreadable ones are ignored.
pub fn reading_data<'a>(files: impl IntoIterator<Item = &'a [u8]>) -> ReadingData {
    let mut data = ReadingData::default();
    for file in files {
        let Some(objects) = parse(file) else { continue };
        for (name, values) in &objects {
            match name.as_str() {
                "lpr" | "updated_lpr" => {
                    if let Some(p) = position(values, name) {
                        let newer = data
                            .last
                            .as_ref()
                            .is_none_or(|old| p.time_ms >= old.time_ms);
                        if newer {
                            data.last = Some(p);
                        }
                    }
                }
                "fpr" => data.furthest = position(values, name).or(data.furthest.take()),
                "timer.model" => {
                    data.total_time_ms = values.get(1).and_then(Value::as_i64);
                    data.total_percent = values.get(3).and_then(Value::as_f64);
                }
                "annotation.cache.object" => data.annotations.extend(annotations(values)),
                _ => {}
            }
        }
    }
    data
}

/// Whether a file in a `.sdr` folder is a KRDS sidecar, by extension.
pub fn is_sidecar(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        ".azw3f", ".azw3r", ".yjf", ".yjr", ".mbs", ".mbp1", ".pdfr", ".pdft",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes KRDS values for tests.
    #[derive(Default)]
    pub struct Writer(pub Vec<u8>);

    impl Writer {
        pub fn new(count: i32) -> Self {
            let mut w = Writer(SIGNATURE.to_vec());
            w.int(1);
            w.int(count);
            w
        }
        pub fn int(&mut self, n: i32) -> &mut Self {
            self.0.push(1);
            self.0.extend_from_slice(&n.to_be_bytes());
            self
        }
        pub fn long(&mut self, n: i64) -> &mut Self {
            self.0.push(2);
            self.0.extend_from_slice(&n.to_be_bytes());
            self
        }
        pub fn double(&mut self, n: f64) -> &mut Self {
            self.0.push(4);
            self.0.extend_from_slice(&n.to_be_bytes());
            self
        }
        pub fn text(&mut self, s: &str) -> &mut Self {
            self.0.push(3);
            self.raw_utf(s);
            self
        }
        fn raw_utf(&mut self, s: &str) {
            self.0.push(0);
            self.0
                .extend_from_slice(&u16::try_from(s.len()).unwrap().to_be_bytes());
            self.0.extend_from_slice(s.as_bytes());
        }
        pub fn begin(&mut self, name: &str) -> &mut Self {
            self.0.push(0xFE);
            self.raw_utf(name);
            self
        }
        pub fn end(&mut self) -> &mut Self {
            self.0.push(0xFF);
            self
        }
    }

    /// An `.azw3f` and an `.azw3r` like a Kindle writes.
    pub fn sample() -> (Vec<u8>, Vec<u8>) {
        let mut f = Writer::new(3);
        f.begin("lpr")
            .int(2)
            .text("4600")
            .long(1_762_000_000_000)
            .end();
        f.begin("fpr")
            .text("6900")
            .long(1_762_000_000_000)
            .int(-1)
            .text("US")
            .text("device")
            .end();
        f.begin("timer.model")
            .long(1)
            .long(600_000)
            .long(1500)
            .double(0.35);
        f.begin("timer.average.calculator")
            .int(0)
            .int(0)
            .int(0)
            .int(0)
            .end();
        f.end();
        let mut r = Writer::new(1);
        r.begin("annotation.cache.object").int(2);
        r.int(1).begin("saved.avl.interval.tree").int(1);
        r.begin("annotation.personal.highlight")
            .text("100")
            .text("180")
            .long(1_762_000_000_000)
            .long(1_762_000_000_000)
            .text("")
            .end();
        r.end();
        r.int(2).begin("saved.avl.interval.tree").int(1);
        r.begin("annotation.personal.note")
            .text("180")
            .text("180")
            .long(1_762_000_001_000)
            .long(1_762_000_001_000)
            .text("")
            .text("A thought")
            .end();
        r.end();
        r.end();
        (f.0, r.0)
    }

    #[test]
    fn reads_position_time_and_annotations() {
        let (f, r) = sample();
        let data = reading_data([f.as_slice(), r.as_slice(), b"junk".as_slice()]);
        assert_eq!(data.last.as_ref().unwrap().offset(), Some(4600));
        assert_eq!(data.last.unwrap().time_ms, Some(1_762_000_000_000));
        assert_eq!(data.furthest.unwrap().offset(), Some(6900));
        assert_eq!(data.total_time_ms, Some(600_000));
        assert_eq!(data.total_percent, Some(0.35));
        assert_eq!(data.annotations.len(), 2);
        assert_eq!(data.annotations[0].kind, "highlight");
        assert_eq!(data.annotations[0].start, "100");
        assert_eq!(data.annotations[1].kind, "note");
        assert_eq!(data.annotations[1].note.as_deref(), Some("A thought"));
    }

    #[test]
    fn rejects_other_data() {
        assert!(parse(b"not krds").is_none());
        let (mut f, _) = sample();
        f.truncate(f.len() - 3);
        assert!(parse(&f).is_none());
        assert!(is_sidecar("Book.AZW3F"));
        assert!(!is_sidecar("Book.apnx"));
    }
}
