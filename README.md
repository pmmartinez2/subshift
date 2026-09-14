# subshift

A command-line tool that shifts every timestamp in an SRT subtitle file by a
fixed offset.

## Why

If a subtitle file is out of sync with a video - a few seconds late because
the release has extra intro footage, or an ad break was cut differently -
every line needs the same correction applied to both the start and end time
of every cue. Doing that by hand in a text editor is tedious and error-prone.
`subshift` reads an SRT file, adds (or subtracts) a millisecond offset from
every timestamp, and writes the result out. Nothing else in the file is
touched: cue numbers, blank lines, and subtitle text pass through unchanged.

## Usage

```
subshift <offset_ms> [file]
```

- `offset_ms` - milliseconds to add to every timestamp. Negative values move
  subtitles earlier. Timestamps are clamped at `00:00:00,000` so they never
  go negative.
- `file` - path to an `.srt` file. Omit it, or pass `-`, to read from stdin.

Output always goes to stdout, so redirect it to save the result.

### Examples

Subtitles start 1.5 seconds too late - delay them further is wrong, so pull
them earlier by 1500 ms:

```
subshift -1500 movie.srt > movie.synced.srt
```

Read from a pipe instead of a file, useful when the subtitles come from
another tool or a download that isn't saved to disk yet:

```
curl -s https://example.com/movie.srt | subshift 2000 > movie.synced.srt
```

Explicit stdin marker also works:

```
cat movie.srt | subshift 2000 - > movie.synced.srt
```

## Building

Requires only the Rust standard library, no external crates.

```
cargo build --release
./target/release/subshift 1000 movie.srt > out.srt
```

## Status

Handles the standard `HH:MM:SS,mmm --> HH:MM:SS,mmm` cue line, including the
optional positioning tags (`X1:... X2:...`) some encoders append after the
end timestamp. Format validation, frame-rate-based resync, and batch
processing of multiple files are not implemented yet.
