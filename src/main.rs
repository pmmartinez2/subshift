use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: subshift <offset_ms> [file]");
        eprintln!("  offset_ms  milliseconds to add to every timestamp (may be negative)");
        eprintln!("  file       .srt file to read; omit or pass '-' to read stdin");
        std::process::exit(2);
    }

    let offset_ms: i64 = match args[1].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "error: offset must be an integer number of milliseconds, got '{}'",
                args[1]
            );
            std::process::exit(2);
        }
    };

    let input = match read_input(args.get(2).map(|s| s.as_str())) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in input.lines() {
        let shifted = shift_line(line, offset_ms);
        if let Err(e) = writeln!(out, "{}", shifted) {
            eprintln!("error: could not write output: {}", e);
            std::process::exit(1);
        }
    }
}

fn read_input(path: Option<&str>) -> Result<String, String> {
    match path {
        None | Some("-") => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("could not read stdin: {}", e))?;
            Ok(buf)
        }
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("could not read '{}': {}", p, e)),
    }
}

/// Rewrites a subtitle cue timing line ("00:00:01,000 --> 00:00:04,000"),
/// leaving every other line (index, text, blank separator) untouched.
fn shift_line(line: &str, offset_ms: i64) -> String {
    let Some(arrow) = line.find("-->") else {
        return line.to_string();
    };
    let start_part = line[..arrow].trim();
    let after_arrow = &line[arrow + 3..];
    let after_trimmed = after_arrow.trim_start();
    let split_at = after_trimmed
        .find(char::is_whitespace)
        .unwrap_or(after_trimmed.len());
    let (end_part, rest) = after_trimmed.split_at(split_at);

    let (Some(start), Some(end)) = (parse_timestamp(start_part), parse_timestamp(end_part)) else {
        return line.to_string();
    };

    format!(
        "{} --> {}{}",
        format_timestamp(shift_ms(start, offset_ms)),
        format_timestamp(shift_ms(end, offset_ms)),
        rest
    )
}

fn shift_ms(ts_ms: i64, offset_ms: i64) -> i64 {
    (ts_ms + offset_ms).max(0)
}

/// Parses "HH:MM:SS,mmm" into total milliseconds.
fn parse_timestamp(s: &str) -> Option<i64> {
    let (hms, ms) = s.split_once(',')?;
    let mut parts = hms.split(':');
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let sec: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let ms: i64 = ms.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

fn format_timestamp(total_ms: i64) -> String {
    let ms = total_ms % 1000;
    let total_sec = total_ms / 1000;
    let s = total_sec % 60;
    let total_min = total_sec / 60;
    let m = total_min % 60;
    let h = total_min / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_round_trip() {
        assert_eq!(parse_timestamp("00:00:01,000"), Some(1000));
        assert_eq!(parse_timestamp("01:02:03,456"), Some(3_723_456));
        assert_eq!(format_timestamp(1000), "00:00:01,000");
        assert_eq!(format_timestamp(3_723_456), "01:02:03,456");
    }

    #[test]
    fn rejects_malformed_timestamps() {
        assert_eq!(parse_timestamp("not a timestamp"), None);
        assert_eq!(parse_timestamp("00:00:01"), None);
        assert_eq!(parse_timestamp("00:00:00:01,000"), None);
    }

    #[test]
    fn shifts_cue_line_forward_and_backward() {
        let line = "00:00:01,000 --> 00:00:04,000";
        assert_eq!(shift_line(line, 1500), "00:00:02,500 --> 00:00:05,500");
        assert_eq!(shift_line(line, -500), "00:00:00,500 --> 00:00:03,500");
    }

    #[test]
    fn clamps_at_zero_instead_of_going_negative() {
        let line = "00:00:01,000 --> 00:00:04,000";
        assert_eq!(shift_line(line, -5000), "00:00:00,000 --> 00:00:00,000");
    }

    #[test]
    fn leaves_non_timing_lines_alone() {
        assert_eq!(shift_line("1", 1000), "1");
        assert_eq!(shift_line("Hello there", 1000), "Hello there");
        assert_eq!(shift_line("", 1000), "");
    }

    #[test]
    fn preserves_trailing_position_tags() {
        let line = "00:00:01,000 --> 00:00:04,000 X1:40 X2:640";
        assert_eq!(
            shift_line(line, 1000),
            "00:00:02,000 --> 00:00:05,000 X1:40 X2:640"
        );
    }
}
