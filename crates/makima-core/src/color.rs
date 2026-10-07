//! sRGB colours and the perceptual ([`Oklab`] / [`Oklch`]) maths used to
//! derive design tokens. Formulas from Björn Ottosson's Oklab reference;
//! single-letter names follow its notation.
#![expect(
    clippy::many_single_char_names,
    reason = "colour maths reads best with the formulas' letters"
)]

use std::fmt;
use std::str::FromStr;

/// An opaque sRGB colour, written `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid colour {0:?}: expected #rrggbb or #rgb")]
pub struct ParseColorError(pub String);

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// WCAG 2 relative luminance, 0 (black) to 1 (white).
    #[must_use]
    pub fn luminance(self) -> f32 {
        let [r, g, b] = self.linear();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// WCAG 2 contrast ratio between two colours, 1 to 21.
    #[must_use]
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Interpolates toward `other` in [`Oklab`]; `t = 0` is `self`, `1` is `other`.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        let (a, b) = (self.oklab(), other.oklab());
        let t = t.clamp(0.0, 1.0);
        Oklab {
            l: a.l + (b.l - a.l) * t,
            a: a.a + (b.a - a.a) * t,
            b: a.b + (b.b - a.b) * t,
        }
        .to_rgb()
    }

    #[must_use]
    pub fn oklab(self) -> Oklab {
        let [r, g, b] = self.linear();
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        Oklab {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }

    #[must_use]
    pub fn oklch(self) -> Oklch {
        let lab = self.oklab();
        Oklch {
            l: lab.l,
            c: lab.a.hypot(lab.b),
            h: lab.b.atan2(lab.a),
        }
    }

    fn linear(self) -> [f32; 3] {
        [self.r, self.g, self.b].map(|channel| {
            let c = f32::from(channel) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || ParseColorError(s.to_owned());
        let hex = s.trim().trim_start_matches('#');
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(error());
        }
        let channel = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16);
        match hex.len() {
            6 => Ok(Self::new(
                channel(0, 2).map_err(|_| error())?,
                channel(2, 2).map_err(|_| error())?,
                channel(4, 2).map_err(|_| error())?,
            )),
            3 => {
                let short = |i| channel(i, 1).map(|v| v * 17).map_err(|_| error());
                Ok(Self::new(short(0)?, short(1)?, short(2)?))
            }
            _ => Err(error()),
        }
    }
}

impl TryFrom<String> for Rgb {
    type Error = ParseColorError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Rgb> for String {
    fn from(color: Rgb) -> Self {
        color.to_string()
    }
}

/// A colour in the Oklab perceptual space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl Oklab {
    /// Converts to sRGB, clamping channels that fall outside the gamut.
    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        let [r, g, b] = self.linear_rgb();
        let encode = |c: f32| {
            let c = c.clamp(0.0, 1.0);
            let v = if c <= 0.003_130_8 {
                12.92 * c
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            };
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "Clamped to 0..=255 first, so the cast can't truncate"
            )]
            let byte = (v * 255.0).round().clamp(0.0, 255.0) as u8;
            byte
        };
        Rgb::new(encode(r), encode(g), encode(b))
    }

    fn linear_rgb(self) -> [f32; 3] {
        let l = (self.l + 0.396_337_78 * self.a + 0.215_803_76 * self.b).powi(3);
        let m = (self.l - 0.105_561_346 * self.a - 0.063_854_17 * self.b).powi(3);
        let s = (self.l - 0.089_484_18 * self.a - 1.291_485_5 * self.b).powi(3);
        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    }

    fn in_gamut(self) -> bool {
        self.linear_rgb()
            .iter()
            .all(|c| (-0.000_1..=1.000_1).contains(c))
    }
}

/// [`Oklab`] in polar form: lightness, chroma, hue (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Oklch {
    /// Converts to sRGB, reducing chroma (never hue or lightness) until the
    /// colour fits the gamut.
    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        let lab = |c: f32| Oklab {
            l: self.l.clamp(0.0, 1.0),
            a: c * self.h.cos(),
            b: c * self.h.sin(),
        };
        if lab(self.c).in_gamut() {
            return lab(self.c).to_rgb();
        }
        let (mut lo, mut hi) = (0.0, self.c);
        for _ in 0..20 {
            let mid = f32::midpoint(lo, hi);
            if lab(mid).in_gamut() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lab(lo).to_rgb()
    }

    #[must_use]
    pub fn with_lightness(self, l: f32) -> Self {
        Self { l, ..self }
    }
}

/// Shifts `color`'s lightness until it has at least `min` contrast against
/// `background`, moving away from the background. Returns the best effort
/// if the target is unreachable.
#[must_use]
pub fn ensure_contrast(color: Rgb, background: Rgb, min: f32) -> Rgb {
    if color.contrast(background) >= min {
        return color;
    }
    let lch = color.oklch();
    let lighter = background.luminance() < 0.5;
    let mut best = color;
    for step in 1..=100u8 {
        let delta = f32::from(step) * 0.01;
        let l = if lighter {
            lch.l + delta
        } else {
            lch.l - delta
        };
        if !(0.0..=1.0).contains(&l) {
            break;
        }
        best = lch.with_lightness(l).to_rgb();
        if best.contrast(background) >= min {
            break;
        }
    }
    best
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!("#1a2B3c".parse::<Rgb>().unwrap().to_string(), "#1a2b3c");
        assert_eq!("fff".parse::<Rgb>().unwrap(), Rgb::new(255, 255, 255));
        assert!("#12345".parse::<Rgb>().is_err());
        assert!("#gggggg".parse::<Rgb>().is_err());
    }

    #[test]
    fn oklab_round_trips() {
        for color in [
            Rgb::new(0, 0, 0),
            Rgb::new(255, 255, 255),
            Rgb::new(16, 16, 16),
            Rgb::new(200, 60, 90),
        ] {
            assert_eq!(color.oklab().to_rgb(), color);
            assert_eq!(color.oklch().to_rgb(), color);
        }
    }

    #[test]
    fn contrast_matches_wcag() {
        let ratio = Rgb::new(0, 0, 0).contrast(Rgb::new(255, 255, 255));
        assert!((ratio - 21.0).abs() < 0.01);
    }

    #[test]
    fn mix_hits_its_endpoints() {
        let (a, b) = (Rgb::new(16, 16, 16), Rgb::new(250, 240, 230));
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
    }

    #[test]
    fn contrast_gate_moves_away_from_the_background() {
        let background = Rgb::new(16, 16, 16);
        let gated = ensure_contrast(Rgb::new(40, 40, 40), background, 4.5);
        assert!(gated.contrast(background) >= 4.5);
        assert!(gated.luminance() > background.luminance());
    }
}
