//! A conversation with one AULA keyboard over a [`Transport`]: one request at a time, each
//! answered before the next is sent.

use std::time::{Duration, Instant};

use thiserror::Error;

use crate::hidraw::{Transport, TransportError};
use crate::proto::aula_ff1b::{PAYLOAD_LEN, REPORT_ID, Read, refused};

/// How long a reply may take. The web driver waits about as long.
pub const REPLY_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the keyboard did not answer {0:?} within {ms} ms", ms = REPLY_TIMEOUT.as_millis())]
    Timeout(Read),
    #[error("refusing to send a {0} frame")]
    Refused(&'static str),
    #[error("the keyboard did not acknowledge a write within {ms} ms", ms = REPLY_TIMEOUT.as_millis())]
    NoAck,
}

pub struct Session<T: Transport> {
    transport: T,
    /// Every request and reply payload, in order, for backups and bug reports.
    pub trace: Vec<(Direction, Vec<u8>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
}

impl<T: Transport> Session<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            trace: Vec::new(),
        }
    }

    /// Sends one read and collects its reply pages (payloads without the report ID).
    pub fn read(&mut self, request: Read) -> Result<Vec<Vec<u8>>, SessionError> {
        let payload = request.payload();
        self.send(&payload)?;

        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut replies: Vec<Vec<u8>> = Vec::new();
        while replies.len() < request.pages() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let Some(report) = self.transport.recv(left)? else {
                break;
            };
            let reply = strip_report_id(report);
            self.trace.push((Direction::In, reply.clone()));
            let page = crate::proto::aula_ff1b::page(&reply);
            if request.answered_by(&reply)
                && !replies
                    .iter()
                    .any(|r| crate::proto::aula_ff1b::page(r) == page)
            {
                replies.push(reply);
            }
        }
        if replies.is_empty() {
            return Err(SessionError::Timeout(request));
        }
        Ok(replies)
    }

    /// Reads a request that has a single reply.
    pub fn read_one(&mut self, request: Read) -> Result<Vec<u8>, SessionError> {
        Ok(self.read(request)?.swap_remove(0))
    }

    /// Sends one write. With `ack`, waits for a reply it accepts; some writes get none.
    pub fn write(
        &mut self,
        payload: &[u8; PAYLOAD_LEN],
        ack: Option<fn(&[u8]) -> bool>,
    ) -> Result<(), SessionError> {
        self.send(payload)?;
        let Some(ack) = ack else { return Ok(()) };
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(SessionError::NoAck);
            }
            let Some(report) = self.transport.recv(left)? else {
                return Err(SessionError::NoAck);
            };
            let reply = strip_report_id(report);
            self.trace.push((Direction::In, reply.clone()));
            if ack(&reply) {
                return Ok(());
            }
        }
    }

    fn send(&mut self, payload: &[u8; PAYLOAD_LEN]) -> Result<(), SessionError> {
        if let Some(why) = refused(payload) {
            return Err(SessionError::Refused(why));
        }
        // Drop reports left over from anything earlier, so they are not taken as the answer.
        while let Some(stale) = self.transport.recv(Duration::ZERO)? {
            self.trace.push((Direction::In, strip_report_id(stale)));
        }
        let mut report = Vec::with_capacity(1 + PAYLOAD_LEN);
        report.push(REPORT_ID);
        report.extend_from_slice(payload);
        self.transport.send(&report)?;
        self.trace.push((Direction::Out, payload.to_vec()));
        Ok(())
    }
}

/// hidraw hands back numbered reports with their ID first.
fn strip_report_id(mut report: Vec<u8>) -> Vec<u8> {
    if report.len() == PAYLOAD_LEN + 1 && report[0] == REPORT_ID {
        report.remove(0);
    }
    report
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::proto::aula_ff1b::tests::*;
    use std::collections::VecDeque;

    /// Answers each request with canned replies, keyed by the request's payload.
    pub struct FakeKeyboard {
        pub answers: Vec<(Read, Vec<Vec<u8>>)>,
        pub pending: VecDeque<Vec<u8>>,
        pub sent: Vec<Vec<u8>>,
    }

    impl FakeKeyboard {
        pub fn new(answers: Vec<(Read, Vec<Vec<u8>>)>) -> Self {
            Self {
                answers,
                pending: VecDeque::new(),
                sent: Vec::new(),
            }
        }
    }

    impl Transport for FakeKeyboard {
        fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
            assert_eq!(report.len(), 64, "reports are always full length");
            assert_eq!(report[0], REPORT_ID);
            self.sent.push(report.to_vec());
            if let Some((_, replies)) = self
                .answers
                .iter()
                .find(|(r, _)| r.payload()[..] == report[1..])
            {
                for reply in replies {
                    let mut with_id = vec![REPORT_ID];
                    with_id.extend(reply);
                    self.pending.push_back(with_id);
                }
            }
            Ok(())
        }

        fn recv(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
            Ok(self.pending.pop_front())
        }
    }

    #[test]
    fn collects_both_identity_pages() {
        let kb = FakeKeyboard::new(vec![(
            Read::Identity,
            vec![frame(IDENTITY_0), frame(IDENTITY_1)],
        )]);
        let mut s = Session::new(kb);
        let pages = s.read(Read::Identity).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].len(), PAYLOAD_LEN);
    }

    #[test]
    fn stale_and_unrelated_reports_are_not_answers() {
        let mut kb = FakeKeyboard::new(vec![(
            Read::PollRate,
            vec![frame(STATUS), frame(POLL_RATE)],
        )]);
        let mut stale = vec![REPORT_ID];
        stale.extend(frame(RANGE));
        kb.pending.push_back(stale);
        let mut s = Session::new(kb);
        assert_eq!(s.read_one(Read::PollRate).unwrap(), frame(POLL_RATE));
        // The stale reply was drained before sending, and logged.
        assert_eq!(
            s.trace.first().map(|(d, p)| (*d, p.clone())),
            Some((Direction::In, frame(RANGE)))
        );
    }

    #[test]
    fn writes_wait_for_their_ack() {
        use crate::proto::aula_ff1b::is_trigger_ack;
        let mut kb = FakeKeyboard::new(Vec::new());
        let mut ack = vec![REPORT_ID];
        ack.extend(frame("21 00 00 00 01"));
        kb.pending.push_back(ack.clone());
        let mut s = Session::new(kb);
        // The pending ack predates the write, so it is drained, and no ack follows.
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[..5].copy_from_slice(&[0x21, 0, 0, 0, 0x18]);
        assert!(matches!(
            s.write(&payload, Some(is_trigger_ack)),
            Err(SessionError::NoAck)
        ));
        // Without an ack to wait for, the write simply goes out.
        assert!(s.write(&payload, None).is_ok());
    }

    #[test]
    fn refused_frames_never_reach_the_keyboard() {
        let mut s = Session::new(FakeKeyboard::new(Vec::new()));
        let mut reset = [0u8; PAYLOAD_LEN];
        reset[..6].copy_from_slice(&[0x14, 0, 0, 0, 0x01, 0x01]);
        assert!(matches!(
            s.write(&reset, None),
            Err(SessionError::Refused("factory reset"))
        ));
        assert!(s.transport.sent.is_empty());
    }

    #[test]
    fn silence_is_a_timeout() {
        let mut s = Session::new(FakeKeyboard::new(Vec::new()));
        assert!(matches!(
            s.read(Read::Lighting),
            Err(SessionError::Timeout(Read::Lighting))
        ));
    }
}
