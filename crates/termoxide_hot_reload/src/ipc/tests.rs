#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{io::Cursor, thread, time::Duration};

use interprocess::local_socket::{GenericNamespaced, Stream, ToNsName, traits::Stream as _};

use super::{
    channel::ChildChannel,
    frame::{HEADER_LEN, MAX_PAYLOAD_LEN, read_frame, write_frame},
    *,
};

const TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn a_frame_reads_back_as_written() {
    let mut buffer = Vec::new();
    write_frame(&mut buffer, b"hello").unwrap();
    write_frame(&mut buffer, b"").unwrap();

    let mut reader = Cursor::new(buffer);
    assert_eq!(read_frame(&mut reader).unwrap(), b"hello");
    assert_eq!(read_frame(&mut reader).unwrap(), b"");
}

#[test]
fn writing_an_oversized_payload_fails_before_writing_anything() {
    let mut buffer = Vec::new();

    let error = write_frame(&mut buffer, &vec![0; MAX_PAYLOAD_LEN + 1]).unwrap_err();

    assert!(matches!(error, IpcError::PayloadTooLarge { .. }), "{error}");
    assert!(buffer.is_empty());
}

#[test]
fn reading_an_oversized_header_fails() {
    let header = (MAX_PAYLOAD_LEN + 1).to_le_bytes();

    let error = read_frame(&mut Cursor::new(header)).unwrap_err();

    assert!(matches!(error, IpcError::PayloadTooLarge { .. }), "{error}");
}

#[test]
fn a_truncated_frame_reads_as_closed() {
    let mut frame = Vec::new();
    write_frame(&mut frame, b"hello").unwrap();

    for len in [0, HEADER_LEN - 1, frame.len() - 1] {
        let error = read_frame(&mut Cursor::new(&frame[..len])).unwrap_err();
        assert!(matches!(error, IpcError::Closed), "{len} bytes: {error}");
    }
}

/// Binds a pipe and connects to it from another thread.
fn connected() -> (HostChannel, ChildChannel) {
    let listener = Listener::bind().unwrap();
    let name = listener.name().to_owned();
    let child = thread::spawn(move || connect(&name, TIMEOUT).unwrap());
    let host = listener.accept(TIMEOUT).unwrap();
    (host, child.join().unwrap())
}

#[test]
fn a_ping_gets_a_pong() {
    let (mut host, mut child) = connected();

    host.send(&Request::Ping).unwrap();
    assert_eq!(child.recv(TIMEOUT).unwrap(), Request::Ping);
    child.send(&Response::Pong).unwrap();

    assert_eq!(host.recv(TIMEOUT).unwrap(), Response::Pong);
}

#[test]
fn accepting_without_a_client_times_out() {
    let error = Listener::bind().unwrap().accept(Duration::from_millis(50)).err().unwrap();

    assert!(matches!(error, IpcError::Timeout), "{error}");
}

#[test]
fn an_unanswered_request_times_out() {
    let (mut host, _child) = connected();

    host.send(&Request::Ping).unwrap();

    let error = host.recv(Duration::from_millis(50)).unwrap_err();
    assert!(matches!(error, IpcError::Timeout), "{error}");
}

#[test]
fn a_closed_peer_reads_as_closed() {
    let listener = Listener::bind().unwrap();
    let name = listener.name().to_ns_name::<GenericNamespaced>().unwrap();
    let peer = Stream::connect(name).unwrap();
    let mut host = listener.accept(TIMEOUT).unwrap();

    drop(peer);

    let error = host.recv(TIMEOUT).unwrap_err();
    assert!(matches!(error, IpcError::Closed), "{error}");
}
