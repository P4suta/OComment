#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Suppression {
    NextLine,
    WholeFile,
    RangeStart,
    RangeEnd,
}

impl Suppression {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::NextLine => "biome-ignore",
            Self::WholeFile => "biome-ignore-all",
            Self::RangeStart => "biome-ignore-start",
            Self::RangeEnd => "biome-ignore-end",
        }
    }
}

pub(crate) fn recognize(keyword: &[u8]) -> Option<Suppression> {
    match keyword {
        b"biome-ignore" => Some(Suppression::NextLine),
        b"biome-ignore-all" => Some(Suppression::WholeFile),
        b"biome-ignore-start" => Some(Suppression::RangeStart),
        b"biome-ignore-end" => Some(Suppression::RangeEnd),
        _ => None,
    }
}
