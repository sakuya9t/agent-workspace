//! Per-session raw-byte ring buffer with monotonic global cursors.
//!
//! `head` = total bytes ever written to the session (from birth, `saturating_add`,
//! never wraps). `tail` = oldest byte still replayable (`head - min(head, cap)`).
//! Cursors are global byte counts, never in-ring offsets. See
//! `docs/asmux-protocol.md` → Cursors & replay.
//!
//! Allocation is fallible (`try_reserve` → [`RingError::AllocFailed`]) and the
//! ring never grows past its declared capacity: the *capacity* is the unit the
//! holder's total-memory cap is accounted in, while the backing store grows
//! lazily up to it.

use std::collections::VecDeque;

/// Why a ring operation failed.
#[derive(Debug, PartialEq, Eq)]
pub enum RingError {
    /// Fallible allocation for new bytes failed (maps to `ALLOC_FAILED`).
    AllocFailed,
}

/// Result of a cursor read against the ring.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    /// `from..from+data.len()` copied out of the ring. `data` may be empty when
    /// `from == head` (caller is already up to date) or shorter than requested
    /// (a `max`-bounded partial read).
    Data { from: u64, data: Vec<u8> },
    /// `from` is older than `tail`: the bytes are gone. `earliest` is the
    /// current `tail` (maps to `BUFFER_GAP` with `earliest_cursor`).
    Gap { earliest: u64 },
    /// `from > head`: a cursor past everything ever produced — client drift
    /// (maps to `INVALID_ARGUMENT`; the server must surface, never clamp).
    Invalid,
}

/// A fixed-capacity byte ring for one session.
pub struct Ring {
    cap: usize,
    buf: VecDeque<u8>,
    head: u64,
}

impl Ring {
    /// Create a ring with `cap` bytes of capacity. `cap` is assumed already
    /// range-checked by the caller (`[RING_MIN_BYTES, RING_MAX_BYTES]`).
    pub fn new(cap: usize) -> Self {
        Ring {
            cap,
            buf: VecDeque::new(),
            head: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Total bytes ever written (monotonic).
    pub fn head(&self) -> u64 {
        self.head
    }

    /// Oldest still-replayable cursor.
    pub fn tail(&self) -> u64 {
        self.head.saturating_sub(self.buf.len() as u64)
    }

    /// Append `data` to the ring, dropping oldest bytes to stay within capacity.
    /// `head` advances by the full `data.len()` even when some bytes are
    /// immediately evicted (a single write larger than the ring).
    pub fn push(&mut self, data: &[u8]) -> Result<(), RingError> {
        if self.cap == 0 || data.is_empty() {
            self.head = self.head.saturating_add(data.len() as u64);
            return Ok(());
        }

        // Only the last `cap` bytes of an oversized write can be retained.
        let keep = data.len().min(self.cap);
        let start = data.len().saturating_sub(keep);
        let tail_slice = match data.get(start..) {
            Some(s) => s,
            None => data, // unreachable given start <= len, but stay total
        };

        // Evict from the front so that current_len + keep <= cap.
        let projected = self.buf.len().saturating_add(keep);
        if projected > self.cap {
            let drop_n = projected.saturating_sub(self.cap);
            drop_front(&mut self.buf, drop_n);
        }

        self.buf
            .try_reserve(keep)
            .map_err(|_| RingError::AllocFailed)?;
        self.buf.extend(tail_slice.iter().copied());

        self.head = self.head.saturating_add(data.len() as u64);
        Ok(())
    }

    /// Copy up to `max` bytes starting at cursor `from`. `max == 0` is treated
    /// as "no explicit bound" and clamped to what is available.
    pub fn read_at(&self, from: u64, max: usize) -> ReadOutcome {
        let head = self.head;
        let tail = self.tail();
        if from > head {
            return ReadOutcome::Invalid;
        }
        if from < tail {
            return ReadOutcome::Gap { earliest: tail };
        }
        if from == head {
            return ReadOutcome::Data {
                from,
                data: Vec::new(),
            };
        }
        let skip = from.saturating_sub(tail) as usize;
        let avail = self.buf.len().saturating_sub(skip);
        let want = if max == 0 { avail } else { avail.min(max) };
        let data = copy_range(&self.buf, skip, want);
        ReadOutcome::Data { from, data }
    }
}

/// Drop `n` bytes from the front of `buf` (saturating at its length).
fn drop_front(buf: &mut VecDeque<u8>, n: usize) {
    // `n` is clamped to the length, so `..n` is always a valid drain range.
    let n = n.min(buf.len());
    drop(buf.drain(..n));
}

/// Copy `n` bytes starting `skip` bytes into `buf`, spanning both ring segments.
fn copy_range(buf: &VecDeque<u8>, skip: usize, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let (a, b) = buf.as_slices();
    let a_len = a.len();
    if skip < a_len {
        let take_a = n.min(a_len.saturating_sub(skip));
        if let Some(s) = a.get(skip..skip.saturating_add(take_a)) {
            out.extend_from_slice(s);
        }
        let rem = n.saturating_sub(take_a);
        if rem > 0 {
            if let Some(s) = b.get(..rem) {
                out.extend_from_slice(s);
            }
        }
    } else {
        let bskip = skip.saturating_sub(a_len);
        if let Some(s) = b.get(bskip..bskip.saturating_add(n)) {
            out.extend_from_slice(s);
        }
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    #[test]
    fn head_tail_advance_and_wrap() {
        let mut r = Ring::new(8);
        r.push(b"abc").unwrap();
        assert_eq!(r.head(), 3);
        assert_eq!(r.tail(), 0);
        r.push(b"defghij").unwrap(); // total 10 > cap 8 -> tail advances to 2
        assert_eq!(r.head(), 10);
        assert_eq!(r.tail(), 2);
    }

    #[test]
    fn read_from_cursor_returns_tail() {
        let mut r = Ring::new(8);
        r.push(b"abcdefghij").unwrap(); // keeps last 8: "cdefghij", tail=2, head=10
        match r.read_at(2, 0) {
            ReadOutcome::Data { from, data } => {
                assert_eq!(from, 2);
                assert_eq!(&data, b"cdefghij");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn read_before_tail_is_gap() {
        let mut r = Ring::new(4);
        r.push(b"abcdef").unwrap(); // keeps "cdef", tail=2, head=6
        assert_eq!(r.read_at(0, 0), ReadOutcome::Gap { earliest: 2 });
        assert_eq!(r.read_at(1, 0), ReadOutcome::Gap { earliest: 2 });
    }

    #[test]
    fn read_past_head_is_invalid() {
        let mut r = Ring::new(8);
        r.push(b"abc").unwrap();
        assert_eq!(r.read_at(4, 0), ReadOutcome::Invalid);
    }

    #[test]
    fn read_at_head_is_empty() {
        let mut r = Ring::new(8);
        r.push(b"abc").unwrap();
        assert_eq!(
            r.read_at(3, 0),
            ReadOutcome::Data {
                from: 3,
                data: Vec::new()
            }
        );
    }

    #[test]
    fn partial_read_respects_max() {
        let mut r = Ring::new(16);
        r.push(b"hello world").unwrap();
        match r.read_at(0, 5) {
            ReadOutcome::Data { from, data } => {
                assert_eq!(from, 0);
                assert_eq!(&data, b"hello");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn read_spanning_wrapped_segments() {
        // Force a wrap so as_slices() returns two non-empty segments.
        let mut r = Ring::new(6);
        r.push(b"abcdef").unwrap(); // buf full: "abcdef"
        r.push(b"gh").unwrap(); // drops "ab", buf now "cdefgh", head=8, tail=2
        match r.read_at(2, 0) {
            ReadOutcome::Data { from, data } => {
                assert_eq!(from, 2);
                assert_eq!(&data, b"cdefgh");
            }
            other => panic!("{other:?}"),
        }
    }
}
