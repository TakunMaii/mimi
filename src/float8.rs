const MAX_FINITE: u8 = 0x7e;
const CANONICAL_NAN: u8 = 0x7f;

pub(crate) fn to_e4m3fn(value: f64) -> u8 {
    if value.is_nan() {
        return CANONICAL_NAN;
    }

    let sign = u8::from(value.is_sign_negative()) << 7;
    let magnitude = value.abs();
    if magnitude.is_infinite() || magnitude >= 448.0 {
        return sign | MAX_FINITE;
    }

    let mut best = 0;
    let mut best_distance = magnitude;
    for encoding in 1..=MAX_FINITE {
        let candidate = f64::from_e4m3fn_magnitude(encoding);
        let distance = (magnitude - candidate).abs();
        if distance < best_distance || (distance == best_distance && encoding & 1 == 0) {
            best = encoding;
            best_distance = distance;
        }
    }
    sign | best
}

pub(crate) fn from_e4m3fn(bits: u8) -> f64 {
    let sign = if bits & 0x80 == 0 { 1.0 } else { -1.0 };
    let magnitude = bits & 0x7f;
    if magnitude == CANONICAL_NAN {
        return f64::NAN;
    }
    sign * f64::from_e4m3fn_magnitude(magnitude)
}

trait E4M3FnMagnitude {
    fn from_e4m3fn_magnitude(bits: u8) -> Self;
}

impl E4M3FnMagnitude for f64 {
    fn from_e4m3fn_magnitude(bits: u8) -> Self {
        let exponent = (bits >> 3) & 0x0f;
        let fraction = bits & 0x07;
        if exponent == 0 {
            f64::from(fraction) * 2f64.powi(-9)
        } else {
            (1.0 + f64::from(fraction) / 8.0) * 2f64.powi(i32::from(exponent) - 7)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{from_e4m3fn, to_e4m3fn};

    #[test]
    fn encodes_and_decodes_e4m3fn_boundaries() {
        for (value, bits) in [
            (0.0, 0x00),
            (2f64.powi(-9), 0x01),
            (2f64.powi(-6), 0x08),
            (1.0, 0x38),
            (1.5, 0x3c),
            (448.0, 0x7e),
            (-1.0, 0xb8),
        ] {
            assert_eq!(to_e4m3fn(value), bits);
            assert_eq!(from_e4m3fn(bits), value);
        }
        assert!(from_e4m3fn(to_e4m3fn(f64::NAN)).is_nan());
        assert_eq!(to_e4m3fn(f64::INFINITY), 0x7e);
        assert_eq!(to_e4m3fn(f64::NEG_INFINITY), 0xfe);
    }

    #[test]
    fn rounds_ties_to_even_and_saturates_at_the_finite_limit() {
        assert_eq!(to_e4m3fn(1.0625), 0x38);
        assert_eq!(to_e4m3fn(1.1875), 0x3a);
        assert_eq!(to_e4m3fn(1000.0), 0x7e);
        assert_eq!(to_e4m3fn(-1000.0), 0xfe);
    }
}
