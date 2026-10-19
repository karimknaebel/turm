use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSize {
    Ratio(u16, u16),
    Content(u16),
}

impl FromStr for PanelSize {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(percent) = value.strip_suffix('%') {
            match percent.parse::<u16>() {
                Ok(percent) if percent <= 100 => Ok(Self::Ratio(percent, 100)),
                _ => Err("expected a percentage between 0% and 100%"),
            }
        } else {
            value.parse().map(Self::Content).map_err(
                |_| "expected a content size between 0 and 65535, or a percentage like 25%",
            )
        }
    }
}

impl PanelSize {
    pub fn outer_size(self, available: u16) -> u16 {
        match self {
            Self::Ratio(size, total) => {
                (u32::from(size) * u32::from(available)).div_ceil(u32::from(total)) as u16
            }
            Self::Content(0) => 0,
            Self::Content(size) => size.saturating_add(2).min(available),
        }
    }

    pub fn resized(self, outer_size: u16, available: u16) -> Self {
        match self {
            Self::Ratio(_, _) => Self::Ratio(outer_size, available),
            Self::Content(_) => Self::Content(outer_size.saturating_sub(2)),
        }
    }
}
