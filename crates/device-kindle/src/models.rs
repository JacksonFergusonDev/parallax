//! Kindle models from their serial numbers. The device codes come from
//! KOReader (`frontend/device/kindle/device.lua`), which cites the MobileRead
//! wiki's Kindle serial number list and NiLuJe's KindleTool. Screen sizes are
//! the published panel resolutions.

use parallax_device::{Screen, ScreenSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub colour: bool,
    /// The screen size is the published one, not a guess.
    pub screen_known: bool,
}

impl Model {
    pub fn screen(&self) -> Screen {
        Screen {
            width: self.width,
            height: self.height,
            colour: self.colour,
            source: if self.screen_known {
                ScreenSource::Model
            } else {
                ScreenSource::Assumed
            },
        }
    }
}

const fn m(name: &'static str, width: u32, height: u32) -> Model {
    Model {
        name,
        width,
        height,
        colour: false,
        screen_known: true,
    }
}

/// Older Kindles: the 3rd and 4th characters of a serial starting B or 9.
const V1: &[(&[&str], Model)] = &[
    (&["02", "03"], m("Kindle 2", 600, 800)),
    (&["04", "05"], m("Kindle DX", 824, 1200)),
    (&["09"], m("Kindle DX Graphite", 824, 1200)),
    (&["08", "06", "0A"], m("Kindle Keyboard", 600, 800)),
    (&["0E", "23"], m("Kindle 4", 600, 800)),
    (&["0F", "11", "10", "12"], m("Kindle Touch", 600, 800)),
    (
        &["24", "1B", "1D", "1F", "1C", "20"],
        m("Kindle Paperwhite", 758, 1024),
    ),
    (
        &[
            "D4", "5A", "D5", "D6", "D7", "D8", "F2", "17", "60", "F4", "F9", "62", "61", "5F",
        ],
        m("Kindle Paperwhite 2", 758, 1024),
    ),
    (&["C6", "DD"], m("Kindle (7th generation)", 600, 800)),
    (
        &["13", "54", "2A", "4F", "52", "53"],
        m("Kindle Voyage", 1072, 1448),
    ),
];

/// Newer Kindles: characters 4 to 6 of the serial.
const V2: &[(&[&str], Model)] = &[
    (
        &[
            "0G1", "0G2", "0G4", "0G5", "0G6", "0G7", "0KB", "0KC", "0KD", "0KE", "0KF", "0KG",
            "0LK", "0LL",
        ],
        m("Kindle Paperwhite 3", 1072, 1448),
    ),
    (
        &["0GC", "0GD", "0GR", "0GS", "0GT", "0GU"],
        m("Kindle Oasis", 1072, 1448),
    ),
    (
        &[
            "0LM", "0LN", "0LP", "0LQ", "0P1", "0P2", "0P6", "0P7", "0P8", "0S1", "0S2", "0S3",
            "0S4", "0S7", "0SA",
        ],
        m("Kindle Oasis 2", 1264, 1680),
    ),
    (
        &["0DU", "0K9", "0KA"],
        m("Kindle (8th generation)", 600, 800),
    ),
    (
        &[
            "0PP", "0T1", "0T2", "0T3", "0T4", "0T5", "0T6", "0T7", "0TJ", "0TK", "0TL", "0TM",
            "0TN", "102", "103", "16Q", "16R", "16S", "16T", "16U", "16V",
        ],
        m("Kindle Paperwhite 4", 1072, 1448),
    ),
    (
        &["10L", "0WF", "0WG", "0WH", "0WJ", "0VB"],
        m("Kindle (10th generation)", 600, 800),
    ),
    (
        &["11L", "0WQ", "0WP", "0WN", "0WM", "0WL"],
        m("Kindle Oasis 3", 1264, 1680),
    ),
    (
        &["1Q0", "1PX", "1VD", "21A", "2BJ", "2DK"],
        m("Kindle Paperwhite 5", 1236, 1648),
    ),
    (
        &["1LG", "219", "2BH"],
        m("Kindle Paperwhite 5 Signature Edition", 1236, 1648),
    ),
    (
        &["22D", "25T", "23A", "2AQ", "2AP", "1XH", "22C"],
        m("Kindle (11th generation)", 1072, 1448),
    ),
    (
        &["27J", "2BL", "263", "227", "2BM", "23L", "23M", "270"],
        m("Kindle Scribe", 1860, 2480),
    ),
    (
        &[
            "3H2", "3H4", "3H6", "3H7", "3H9", "3JT", "3J6", "455", "456", "4EP", "34X", "3HB",
            "5R6", "5R7", "5R8", "5R9", "5RA", "5RB", "65R", "65S", "65T",
        ],
        Model {
            colour: true,
            ..m("Kindle Colorsoft", 1264, 1680)
        },
    ),
    (
        &["A89", "3L2", "3L3", "3L4", "3L5", "3L6", "3KM"],
        m("Kindle (12th generation)", 1072, 1448),
    ),
    (
        &[
            "33W", "33X", "346", "349", "3H3", "3H5", "3H8", "3HA", "3J5", "3JS",
        ],
        m("Kindle Paperwhite 6", 1264, 1680),
    ),
    (
        &["3V0", "3V1", "3X5", "3UV", "3X4", "3X3", "41E", "410"],
        m("Kindle Scribe", 1860, 2480),
    ),
    (
        &["4PG", "4PE", "4PL", "4F8", "4FA", "454"],
        Model {
            screen_known: false,
            ..m("Kindle Scribe 3", 1860, 2480)
        },
    ),
    (
        &["4VX", "4PF", "4PH", "4F9", "4FB", "46P"],
        Model {
            colour: true,
            screen_known: false,
            ..m("Kindle Scribe Colorsoft", 1860, 2480)
        },
    ),
];

/// Models that have been through the Kindle hardware checks
/// (`engineering/hardware-checks.md`), by [`Model::name`]. None yet.
pub const TESTED_MODELS: &[&str] = &[];

/// A Kindle whose serial isn't in the table.
pub const UNKNOWN: Model = Model {
    name: "Kindle",
    width: 1072,
    height: 1448,
    colour: false,
    screen_known: false,
};

/// The model for a serial number, if it's a known one.
pub fn from_serial(serial: &str) -> Option<Model> {
    let serial = serial.trim().to_ascii_uppercase();
    let lookup = |table: &[(&[&str], Model)], code: &str| {
        table
            .iter()
            .find(|(codes, _)| codes.contains(&code))
            .map(|(_, model)| *model)
    };
    if serial.starts_with('B') || serial.starts_with('9') {
        lookup(V1, serial.get(2..4)?)
    } else {
        lookup(V2, serial.get(3..6)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_serials() {
        assert_eq!(
            from_serial("B0240000000000").unwrap().name,
            "Kindle Paperwhite"
        );
        assert_eq!(
            from_serial("G003H20000000000").unwrap().name,
            "Kindle Colorsoft"
        );
        assert!(from_serial("G003H20000000000").unwrap().colour);
        assert_eq!(
            from_serial("g0033w0000000000").unwrap().name,
            "Kindle Paperwhite 6"
        );
        assert_eq!(
            from_serial("G004PG0000000000").unwrap().screen().source,
            ScreenSource::Assumed
        );
        assert!(from_serial("ZZZ999").is_none());
        assert!(from_serial("").is_none());
    }
}
