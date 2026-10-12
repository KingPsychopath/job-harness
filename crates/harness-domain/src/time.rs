//! Parse historical ISO input forms. Clocks and current time belong to the host.
use crate::{HarnessError as Error, Result};

/// Accept ISO forms historically admitted by datetime.fromisoformat, including
/// basic dates, week dates, a space separator, short times and offset seconds.
pub fn iso_datetime(value: &str) -> Result<(chrono::NaiveDateTime, Option<chrono::FixedOffset>)> {
    use chrono::{FixedOffset, NaiveDate, NaiveTime};
    let date_pattern =
        regex::Regex::new(r"^(\d{4}-\d{2}-\d{2}|\d{8}|\d{4}-?W\d{2}(?:-?\d)?)(?:.(.*))?$").unwrap();
    let caps = date_pattern
        .captures(value)
        .ok_or_else(|| Error::new("invalid_arguments", "ISO date or datetime required"))?;
    let mut date_text = caps[1].to_owned();
    if date_text.contains('W') {
        date_text = date_text.replace('-', "");
        if date_text.len() == 7 {
            date_text.push('1');
        }
    }
    let date = ["%Y-%m-%d", "%Y%m%d", "%GW%V%u"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(&date_text, format).ok())
        .ok_or_else(|| Error::new("invalid_arguments", "ISO date required"))?;
    let mut time_text = caps
        .get(2)
        .map(|v| v.as_str().to_owned())
        .unwrap_or_default();
    if time_text.is_empty() {
        return Ok((
            date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            None,
        ));
    }
    let mut offset = None;
    if time_text.ends_with('Z') {
        time_text.pop();
        offset = FixedOffset::east_opt(0);
    } else {
        let pattern =
            regex::Regex::new(r"([+-])(\d{2})(?::?(\d{2}))?(?::?(\d{2})(?:[.,]\d+)?)?$").unwrap();
        if let Some(c) = pattern.captures(&time_text) {
            let start = c.get(0).unwrap().start();
            let hour = c[2].parse::<i32>().unwrap_or(100);
            let minute = c
                .get(3)
                .map(|v| v.as_str().parse::<i32>().unwrap_or(100))
                .unwrap_or(0);
            let second = c
                .get(4)
                .map(|v| v.as_str().parse::<i32>().unwrap_or(100))
                .unwrap_or(0);
            if hour > 23 || minute > 59 || second > 59 {
                return Err(Error::new(
                    "invalid_arguments",
                    "Invalid ISO timezone offset",
                ));
            }
            let seconds = (hour * 3600 + minute * 60 + second) * if &c[1] == "-" { -1 } else { 1 };
            offset = FixedOffset::east_opt(seconds);
            time_text.truncate(start);
        }
    }
    let time_pattern =
        regex::Regex::new(r"^(\d{2})(?::?(\d{2}))?(?::?(\d{2}))?(?:[.,](\d+))?$").unwrap();
    let c = time_pattern
        .captures(&time_text)
        .ok_or_else(|| Error::new("invalid_arguments", "Invalid ISO time"))?;
    let hour = c[1].parse::<u32>().unwrap_or(100);
    let minute = c
        .get(2)
        .map(|v| v.as_str().parse::<u32>().unwrap_or(100))
        .unwrap_or(0);
    let second = c
        .get(3)
        .map(|v| v.as_str().parse::<u32>().unwrap_or(100))
        .unwrap_or(0);
    let fraction = c
        .get(4)
        .map(|v| {
            format!("{:0<9}", &v.as_str()[..v.as_str().len().min(9)])
                .parse::<u32>()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let time = NaiveTime::from_hms_nano_opt(hour, minute, second, fraction)
        .ok_or_else(|| Error::new("invalid_arguments", "Invalid ISO time"))?;
    Ok((date.and_time(time), offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_historical_forms_and_timezone_is_explicit() {
        for value in [
            "2026-10-12",
            "20261012",
            "2026-W42-1",
            "2026W421",
            "2026-10-12 11:00:00",
            "2026-10-12T11",
            "2026-10-12T110000+0100",
            "2026-10-12T11:00:00Z",
            "2026-10-12T11:00:00+01:00:00",
        ] {
            assert!(iso_datetime(value).is_ok(), "{value}");
        }
        assert!(iso_datetime("2026-99-12").is_err());
        assert!(iso_datetime("2026-10-12T11:00:00").unwrap().1.is_none());
        assert!(
            iso_datetime("2026-10-12T11:00:00+01:00")
                .unwrap()
                .1
                .is_some()
        );
    }
}
