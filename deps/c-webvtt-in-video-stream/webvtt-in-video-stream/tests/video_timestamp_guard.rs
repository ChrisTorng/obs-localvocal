use std::{io, time::Duration};
use video_bytestream_tools::webvtt::{WebvttTrack, WebvttWrite};
use webvtt_in_video_stream::{WebvttMuxerBuilder, WebvttString};

#[derive(Default)]
struct TestWriter {
    headers: usize,
    payload_offsets: Vec<Duration>,
}

impl WebvttWrite for TestWriter {
    fn write_webvtt_header(
        &mut self,
        _max_latency_to_video: Duration,
        _send_frequency_hz: u8,
        _subtitle_tracks: &[WebvttTrack],
    ) -> io::Result<()> {
        self.headers += 1;
        Ok(())
    }

    fn write_webvtt_payload(
        &mut self,
        _track_index: u8,
        _chunk_number: u64,
        _chunk_version: u8,
        video_offset: Duration,
        _webvtt_payload: &str,
    ) -> io::Result<()> {
        self.payload_offsets.push(video_offset);
        Ok(())
    }
}

#[test]
fn waits_for_video_timestamp_before_muxing_next_chunk() {
    // Reproduces LocalVocal #311 at 25 fps, 2 Hz, 60 ms latency:
    // the next WebVTT chunk is at 500 ms while the preceding video frame is at 480 ms.
    // The look-ahead guard allows 480 ms through, so subtracting 500 ms from 480 ms
    // would underflow std::time::Duration and abort the process in release builds.
    let mut builder =
        WebvttMuxerBuilder::new(Duration::from_millis(60), 2, Duration::from_millis(40));

    assert!(builder
        .add_track(
            false,
            false,
            false,
            WebvttString::from_string("English".into()).unwrap(),
            WebvttString::from_string("en".into()).unwrap(),
            None,
            None,
        )
        .is_ok());

    let muxer = builder.create_muxer();
    let mut writer = TestWriter::default();

    assert!(muxer
        .try_mux_into_bytestream(Duration::ZERO, true, &mut writer)
        .unwrap());
    assert_eq!(writer.headers, 1);
    assert_eq!(writer.payload_offsets, vec![Duration::ZERO]);

    // 480 ms is still before the 500 ms WebVTT chunk timestamp. It must be deferred.
    assert!(!muxer
        .try_mux_into_bytestream(Duration::from_millis(480), false, &mut writer)
        .unwrap());
    assert_eq!(writer.payload_offsets, vec![Duration::ZERO]);

    // The next 25 fps frame is at 520 ms, so the 500 ms chunk can now be emitted.
    assert!(muxer
        .try_mux_into_bytestream(Duration::from_millis(520), false, &mut writer)
        .unwrap());
    assert_eq!(
        writer.payload_offsets,
        vec![Duration::ZERO, Duration::from_millis(20)]
    );
}
