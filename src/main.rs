use std::io::{Read, Write};

fn print_usage_and_exit() -> ! {
    eprintln!("usage: subshift <offset_ms> [--scale <factor>] [file]");
    eprintln!("  offset_ms  milliseconds to add to every timestamp (may be negative)");
    eprintln!("  --scale    multiply every timestamp by this factor before the offset is added;");
    eprintln!("             use target_fps / source_fps to retime for a frame-rate change");
    eprintln!("  file       .srt file to read; omit or pass '-' to read stdin");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut scale: f64 = 1.0;
    let mut positional: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--scale" {
            let Some(value) = args.get(i + 1) else {
                eprintln!("error: --scale requires a value");
                std::process::exit(2);
            };
            scale = match value.parse() {
                Ok(v) if v > 0.0 => v,
                _ => {
                    eprintln!("error: --scale must be a positive number, got '{}'", value);
                    std::process::exit(2);
                }
            };
            i += 2;
        } else {
            positional.push(args[i].as_str());
            i += 1;
        }
    }

    if positional.is_empty() || positional.len() > 2 {
        print_usage_and_exit();
    }

    let offset_ms: i64 = match positional[0].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "error: offset must be an integer number of milliseconds, got '{}'",
                positional[0]
            );
            std::process::exit(2);
        }
    };

    let input = match read_input(positional.get(1).copied()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let errors = validate_structure(&input);
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error: {}", e);
        }
        std::process::exit(1);
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in input.lines() {
        let shifted = shift_line(line, scale, offset_ms);
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

/// Splits a timing line into its start timestamp, end timestamp, and
/// whatever trails the end timestamp (e.g. VobSub-style position tags),
/// without checking that the timestamps themselves are valid.
fn split_timing_line(line: &str) -> Option<(&str, &str, &str)> {
    let arrow = line.find("-->")?;
    let start_part = line[..arrow].trim();
    let after_arrow = &line[arrow + 3..];
    let after_trimmed = after_arrow.trim_start();
    let split_at = after_trimmed
        .find(char::is_whitespace)
        .unwrap_or(after_trimmed.len());
    let (end_part, rest) = after_trimmed.split_at(split_at);
    Some((start_part, end_part, rest))
}

fn is_timing_line(line: &str) -> bool {
    match split_timing_line(line) {
        Some((start, end, _)) => parse_timestamp(start).is_some() && parse_timestamp(end).is_some(),
        None => false,
    }
}

/// Rewrites a subtitle cue timing line ("00:00:01,000 --> 00:00:04,000"),
/// leaving every other line (index, text, blank separator) untouched.
fn shift_line(line: &str, scale: f64, offset_ms: i64) -> String {
    let Some((start_part, end_part, rest)) = split_timing_line(line) else {
        return line.to_string();
    };

    let (Some(start), Some(end)) = (parse_timestamp(start_part), parse_timestamp(end_part)) else {
        return line.to_string();
    };

    format!(
        "{} --> {}{}",
        format_timestamp(shift_ms(start, scale, offset_ms)),
        format_timestamp(shift_ms(end, scale, offset_ms)),
        rest
    )
}

/// Walks the input as a sequence of SRT cues (index line, timing line, one
/// or more text lines, blank separator) and collects a human-readable error
/// for every place the structure breaks down, tagged with the 1-based line
/// number so the caller can find it in the original file.
fn validate_structure(input: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let mut lines = input.lines().enumerate().map(|(i, l)| (i + 1, l)).peekable();

    while let Some(&(_, line)) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
            continue;
        }

        let (index_no, index_line) = lines.next().unwrap();
        if index_line.trim().parse::<u64>().is_err() {
            errors.push(format!(
                "line {}: expected a cue number, found '{}'",
                index_no, index_line
            ));
        }

        match lines.next() {
            Some((_, timing_line)) if is_timing_line(timing_line) => {}
            Some((timing_no, timing_line)) => {
                errors.push(format!(
                    "line {}: expected a timing line (HH:MM:SS,mmm --> HH:MM:SS,mmm), found '{}'",
                    timing_no, timing_line
                ));
            }
            None => {
                errors.push(format!(
                    "line {}: cue '{}' is missing a timing line",
                    index_no,
                    index_line.trim()
                ));
                break;
            }
        }

        let mut has_text = false;
        while let Some(&(_, l)) = lines.peek() {
            if l.trim().is_empty() {
                break;
            }
            has_text = true;
            lines.next();
        }
        if !has_text {
            errors.push(format!(
                "line {}: cue '{}' has no subtitle text",
                index_no,
                index_line.trim()
            ));
        }
    }

    errors
}

fn shift_ms(ts_ms: i64, scale: f64, offset_ms: i64) -> i64 {
    let scaled = (ts_ms as f64 * scale).round() as i64;
    (scaled + offset_ms).max(0)
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
        assert_eq!(shift_line(line, 1.0, 1500), "00:00:02,500 --> 00:00:05,500");
        assert_eq!(shift_line(line, 1.0, -500), "00:00:00,500 --> 00:00:03,500");
    }

    #[test]
    fn clamps_at_zero_instead_of_going_negative() {
        let line = "00:00:01,000 --> 00:00:04,000";
        assert_eq!(shift_line(line, 1.0, -5000), "00:00:00,000 --> 00:00:00,000");
    }

    #[test]
    fn leaves_non_timing_lines_alone() {
        assert_eq!(shift_line("1", 1.0, 1000), "1");
        assert_eq!(shift_line("Hello there", 1.0, 1000), "Hello there");
        assert_eq!(shift_line("", 1.0, 1000), "");
    }

    #[test]
    fn preserves_trailing_position_tags() {
        let line = "00:00:01,000 --> 00:00:04,000 X1:40 X2:640";
        assert_eq!(
            shift_line(line, 1.0, 1000),
            "00:00:02,000 --> 00:00:05,000 X1:40 X2:640"
        );
    }

    #[test]
    fn scales_timestamps_before_applying_offset() {
        // 25 fps track that was authored as if it were 23.976 fps: stretch
        // every timestamp by 25/23.976 to match the faster frame rate.
        let line = "00:01:00,000 --> 00:02:00,000";
        assert_eq!(
            shift_line(line, 25.0 / 23.976, 0),
            "00:01:02,563 --> 00:02:05,125"
        );
    }

    #[test]
    fn combines_scale_and_offset() {
        let line = "00:00:10,000 --> 00:00:20,000";
        assert_eq!(shift_line(line, 2.0, 500), "00:00:20,500 --> 00:00:40,500");
    }

    #[test]
    fn accepts_well_formed_srt() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello\n\n2\n00:00:05,000 --> 00:00:06,000\nWorld\n";
        assert_eq!(validate_structure(srt), Vec::<String>::new());
    }

    #[test]
    fn accepts_multi_line_cue_text_and_missing_trailing_blank() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello\nthere\n\n2\n00:00:05,000 --> 00:00:06,000\nWorld";
        assert_eq!(validate_structure(srt), Vec::<String>::new());
    }

    #[test]
    fn flags_non_numeric_cue_index() {
        let srt = "one\n00:00:01,000 --> 00:00:04,000\nHello\n";
        let errors = validate_structure(srt);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("line 1:"));
    }

    #[test]
    fn flags_malformed_timing_line() {
        let srt = "1\n00:00:01,000 -> 00:00:04,000\nHello\n";
        let errors = validate_structure(srt);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("line 2:"));
    }

    #[test]
    fn flags_cue_with_no_text() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\n\n2\n00:00:05,000 --> 00:00:06,000\nWorld\n";
        let errors = validate_structure(srt);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("line 1:"));
        assert!(errors[0].contains("no subtitle text"));
    }

    #[test]
    fn flags_cue_missing_timing_line_at_eof() {
        let srt = "1\n";
        let errors = validate_structure(srt);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing a timing line"));
    }
}
