use std::time::Duration;

use anyhow::bail;

/// Fractional digits beyond this are below nanosecond resolution for every
/// supported unit, so they are truncated before doing integer arithmetic.
const MAX_FRAC_DIGITS: usize = 15;

/// Parse a duration string such as `1.43ms`, `430µs`, `.43s`, or `2h 37min`.
///
/// Replaces the unmaintained `parse_duration` crate (RUSTSEC-2021-0041), which
/// silently dropped leading decimal points (`.43ms` parsed as 43ms) and did
/// not recognize the micro sign (U+00B5) that `Duration`'s `Debug` output
/// uses for microseconds.
///
/// Accepts one or more `<number><unit>` segments separated by optional
/// whitespace. Numbers may have a fractional part, with or without a leading
/// zero; sub-nanosecond precision is truncated. Units are case-insensitive:
/// ns, us/µs/μs, ms, s, m/min, h, d, and their long forms. Calendar units
/// (weeks, months, years) are intentionally unsupported, so `M` is minutes,
/// not months.
pub fn parse(input: &str) -> anyhow::Result<Duration> {
    let mut rest = input.trim();
    if rest.is_empty() {
        bail!("empty duration string");
    }

    let mut total_ns: u128 = 0;

    while !rest.is_empty() {
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let (num, tail) = rest.split_at(num_end);

        let (int_part, frac_part) = match num.split_once('.') {
            Some((int, frac)) => (int, frac),
            None => (num, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            bail!("expected a number at {rest:?} in duration {input:?}");
        }
        if frac_part.contains('.') {
            bail!("malformed number {num:?} in duration {input:?}");
        }

        let tail = tail.trim_start();
        let unit_end = tail
            .find(|c: char| c.is_ascii_digit() || c.is_whitespace() || c == '.')
            .unwrap_or(tail.len());
        let (unit, next) = tail.split_at(unit_end);

        let unit_ns: u128 = match unit.to_lowercase().as_str() {
            "ns" | "nanos" | "nanosecond" | "nanoseconds" => 1,
            "us" | "µs" | "μs" | "micros" | "microsecond" | "microseconds" => 1_000,
            "ms" | "millis" | "millisecond" | "milliseconds" => 1_000_000,
            "s" | "sec" | "secs" | "second" | "seconds" => 1_000_000_000,
            "m" | "min" | "mins" | "minute" | "minutes" => 60 * 1_000_000_000,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600 * 1_000_000_000,
            "d" | "day" | "days" => 86_400 * 1_000_000_000,
            "" => bail!("no unit found for the value {num:?} in duration {input:?}"),
            _ => bail!("unknown unit {unit:?} in duration {input:?}"),
        };

        let int: u128 = if int_part.is_empty() {
            0
        } else {
            int_part
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid number {num:?} in duration {input:?}: {e}"))?
        };
        let frac_part = &frac_part[..frac_part.len().min(MAX_FRAC_DIGITS)];
        let frac: u128 = if frac_part.is_empty() {
            0
        } else {
            frac_part
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid number {num:?} in duration {input:?}: {e}"))?
        };

        total_ns = int
            .checked_mul(unit_ns)
            .and_then(|ns| ns.checked_add(frac * unit_ns / 10u128.pow(frac_part.len() as u32)))
            .and_then(|ns| total_ns.checked_add(ns))
            .ok_or_else(|| anyhow::anyhow!("duration {input:?} is too large"))?;

        rest = next.trim_start();
    }

    let secs = u64::try_from(total_ns / 1_000_000_000)
        .map_err(|_| anyhow::anyhow!("duration {input:?} is too large"))?;
    Ok(Duration::new(secs, (total_ns % 1_000_000_000) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_unit_values() {
        let cases = [
            (".43ms", Duration::from_micros(430)),
            ("0.43ms", Duration::from_micros(430)),
            ("1.43ms", Duration::new(0, 1_430_000)),
            ("43ms", Duration::from_millis(43)),
            ("430µs", Duration::from_micros(430)),
            ("430μs", Duration::from_micros(430)),
            ("430us", Duration::from_micros(430)),
            ("23ns", Duration::from_nanos(23)),
            ("1.5s", Duration::from_millis(1500)),
            ("0.1s", Duration::from_millis(100)),
            ("1.5h", Duration::from_secs(5400)),
            ("1.25m", Duration::from_secs(75)),
            ("12M", Duration::from_secs(720)),
            ("43MS", Duration::from_millis(43)),
            ("007ms", Duration::from_millis(7)),
            ("00.43ms", Duration::from_micros(430)),
            ("0.043ms", Duration::from_micros(43)),
            (" 5s ", Duration::from_secs(5)),
            ("1.5 s", Duration::from_millis(1500)),
        ];
        for (input, expected) in cases {
            assert_eq!(parse(input).unwrap(), expected, "input: {input:?}");
        }
    }

    #[test]
    fn parses_compound_values() {
        let cases = [
            ("2h 37min", Duration::from_secs(9420)),
            ("1m30s", Duration::from_secs(90)),
            ("1d", Duration::from_secs(86_400)),
            ("1h 2m 3s 4ms", Duration::new(3723, 4_000_000)),
            ("1s 1s", Duration::from_secs(2)),
            ("30s 1m", Duration::from_secs(90)),
            ("10.5s4m", Duration::from_millis(250_500)),
            ("1h2m3s4ms5us6ns", Duration::new(3723, 4_005_006)),
            ("39h9m14.425s", Duration::new(140_954, 425_000_000)),
            ("9223372036s854ms775us807ns", Duration::new(9_223_372_036, 854_775_807)),
        ];
        for (input, expected) in cases {
            assert_eq!(parse(input).unwrap(), expected, "input: {input:?}");
        }
    }

    #[test]
    fn truncates_below_nanosecond_precision() {
        assert_eq!(parse("0.5ns").unwrap(), Duration::ZERO);
        assert_eq!(parse("0.9999999999s").unwrap(), Duration::new(0, 999_999_999));
        assert_eq!(
            parse("0.123456789123456789123456789s").unwrap(),
            Duration::new(0, 123_456_789)
        );
    }

    #[test]
    fn rejects_invalid_inputs() {
        let cases = [
            "", "   ", "5", "ms", ".", ".ms", "1.4.3ms", "1..5s", "..5s", "5..s", "-5s",
            "1e3ms", "43xs", "1h30", "+5s", "1s,500ms", "5s-3s", "٤٣ms",
            "99999999999999999999999999999999999999999999h",
            // bare zeros (Go accepts these; we require a unit)
            "0", "-0", "+0",
            // calendar units and other-language formats
            "1w", "1y", "1month", "PT1H30M", "1:23",
        ];
        for input in cases {
            assert!(parse(input).is_err(), "input {input:?} should be rejected");
        }
    }

    #[test]
    fn accepts_all_unit_aliases() {
        let families: &[(&[&str], Duration)] = &[
            (&["ns", "nanos", "nanosecond", "nanoseconds"], Duration::from_nanos(1)),
            (&["us", "µs", "μs", "micros", "microsecond", "microseconds"], Duration::from_micros(1)),
            (&["ms", "millis", "millisecond", "milliseconds"], Duration::from_millis(1)),
            (&["s", "sec", "secs", "second", "seconds"], Duration::from_secs(1)),
            (&["m", "min", "mins", "minute", "minutes"], Duration::from_secs(60)),
            (&["h", "hr", "hrs", "hour", "hours"], Duration::from_secs(3600)),
            (&["d", "day", "days"], Duration::from_secs(86_400)),
        ];
        for (aliases, expected) in families {
            for alias in *aliases {
                assert_eq!(parse(&format!("1{alias}")).unwrap(), *expected, "unit: {alias}");
            }
        }
    }

    #[test]
    fn handles_u64_seconds_boundary() {
        assert_eq!(
            parse("18446744073709551615s").unwrap(),
            Duration::new(u64::MAX, 0)
        );
        assert!(parse("18446744073709551616s").is_err());
        assert_eq!(parse(&format!("{:?}", Duration::MAX)).unwrap(), Duration::MAX);
    }

    /// Precision cases from Go's `time.ParseDuration` test table. Integer
    /// arithmetic keeps these exact where float64 would round: 2^53+1 ns is
    /// not representable in a double, and Go's fraction handling is float-
    /// based, so Go reports `0.3333333333333333333h` as exactly 20min while
    /// the true value truncated to nanoseconds is 1ns short of it.
    #[test]
    fn go_parse_duration_precision_cases() {
        let cases = [
            ("9007199254740993ns", Duration::from_nanos(9_007_199_254_740_993)),
            ("9223372036854775.807us", Duration::new(9_223_372_036, 854_775_807)),
            ("0.100000000000000000000h", Duration::from_secs(360)),
            ("0.830103483285477580700h", Duration::new(2988, 372_539_827)),
            ("0.3333333333333333333h", Duration::new(1199, 999_999_999)),
            // overflows Go's int64 nanoseconds; fits comfortably in ours
            ("3000000h", Duration::from_secs(10_800_000_000)),
        ];
        for (input, expected) in cases {
            assert_eq!(parse(input).unwrap(), expected, "input: {input:?}");
        }
    }

    /// Deliberately lenient corners, locked in so a refactor doesn't change
    /// them silently: an empty fraction after a dot is zero, and a dot right
    /// after a unit starts a new segment.
    #[test]
    fn documented_leniencies() {
        assert_eq!(parse("5.s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse("1s.5ms").unwrap(), Duration::new(1, 500_000));
    }

    /// Any `Duration` formatted with `{:?}` must parse back to itself. This is
    /// the whole input space produced by Rust-based samplers that format
    /// timings with `Debug`.
    #[test]
    fn round_trips_rust_debug_format() {
        let mut ns_values = vec![0u64, 1, 999, 1_000, 1_001, 430_000, 1_430_000, 1_500_000_000];
        for exp in 0..=19 {
            for mantissa in [1u64, 3, 7, 23, 43, 143, 999, 999_999_937] {
                if let Some(ns) = mantissa.checked_mul(10u64.pow(exp)) {
                    ns_values.push(ns);
                }
            }
        }
        for ns in ns_values {
            let duration = Duration::from_nanos(ns);
            let formatted = format!("{duration:?}");
            assert_eq!(
                parse(&formatted).unwrap(),
                duration,
                "failed to round-trip {formatted:?}"
            );
        }
    }
}
