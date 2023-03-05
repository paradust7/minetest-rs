use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

use super::util::rel_to_abs;
use crate::wire::packet::AckBody;
use crate::wire::packet::InnerBody;
use crate::wire::packet::PacketBody;
use crate::wire::packet::SEQNUM_INITIAL;

//const MIN_RELIABLE_WINDOW_SIZE: u16 = 0x40; // 64
const START_RELIABLE_WINDOW_SIZE: u16 = 0x400; // 1024

#[cfg(test)]
const MAX_RELIABLE_WINDOW_SIZE: u16 = 0x8000; // 32768

//const RESEND_TIMEOUT_MIN_MS: u64 = 100;
const RESEND_TIMEOUT_START_MS: u64 = 500;
//const RESEND_TIMEOUT_MAX_MS: u64 = 3000;
const RESEND_RESOLUTION: Duration = Duration::from_millis(20);

pub struct ReliableSender {
    // Next reliable send seqnum
    next_seqnum: u64,
    window_size: u16,

    // Packets that have yet to be sent at all
    // These are not in the buffer yet
    queued: VecDeque<(u64, PacketBody)>,

    // Sent packets that haven't yet been ack'd
    // seq num -> packet
    buffer: BTreeMap<u64, PacketBody>,

    // TODO(paradust): Use a better data structure for this
    timeouts: BTreeSet<(Instant, u64)>,
    resend_timeout: Duration,
}

impl ReliableSender {
    pub fn new() -> Self {
        ReliableSender {
            next_seqnum: SEQNUM_INITIAL as u64,
            window_size: START_RELIABLE_WINDOW_SIZE,
            buffer: BTreeMap::new(),
            timeouts: BTreeSet::new(),
            resend_timeout: Duration::from_millis(RESEND_TIMEOUT_START_MS),
            queued: VecDeque::new(),
        }
    }

    pub fn process_ack(&mut self, ack: AckBody) {
        let unacked_base = match self.oldest_unacked() {
            Some(unacked_base) => unacked_base,
            None => {
                return;
            }
        };
        let seqnum = rel_to_abs(unacked_base, ack.seqnum);
        self.buffer.remove(&seqnum);
    }

    /// Push a packet for reliable send.
    pub fn push(&mut self, body: InnerBody) {
        let seqnum = self.next_seqnum;
        self.next_seqnum += 1;
        let body = body.into_reliable(seqnum as u16);
        self.queued.push_back((seqnum, body));
    }

    fn oldest_unacked(&self) -> Option<u64> {
        self.buffer.first_key_value().map(|(seqnum, _)| *seqnum)
    }

    fn safe_to_transmit(&self, seqnum: u64) -> bool {
        match self.oldest_unacked() {
            Some(unacked_seqnum) => seqnum < (unacked_seqnum + (self.window_size as u64)),
            None => true,
        }
    }

    pub fn next_timeout(&self) -> Option<Instant> {
        match self.timeouts.first() {
            Some((when, _)) => Some(*when + RESEND_RESOLUTION),
            None => None,
        }
    }

    /// Pop a single packet for immediate transmission.
    ///
    /// This should be repeatedly called to exhaustion every time there's
    /// a push or when a timeout occurs.
    ///
    /// For the timeout logic to be correct, the returned PacketBody must be sent right away.
    ///
    /// When the send window has been exhausted, this will return None, even if there
    /// is more send, when the send window has been exhausted.
    ///
    /// TODO(paradust): Iterator to make this more efficient
    #[must_use]
    pub fn pop(&mut self, now: Instant) -> Option<PacketBody> {
        // Prioritize expired resends before making new sends
        self.pop_resend(now).or_else(|| self.pop_queued(now))
    }

    fn pop_queued(&mut self, now: Instant) -> Option<PacketBody> {
        let safe = match self.queued.front() {
            Some((seqnum, _)) => self.safe_to_transmit(*seqnum),
            None => false,
        };
        if !safe {
            return None;
        }
        match self.queued.pop_front() {
            Some((seqnum, b)) => {
                self.buffer.insert(seqnum, PacketBody::clone(&b));
                self.timeouts.insert((now + self.resend_timeout, seqnum));
                Some(b)
            }
            None => None,
        }
    }

    fn pop_resend(&mut self, now: Instant) -> Option<PacketBody> {
        // Keep draining while either:
        //  - The timeout is expired
        //  OR
        //  - The packet is not in the buffer (it has already been ack'd)
        //
        // This will prevent unnecessary timers from being set.
        loop {
            match self.timeouts.pop_first() {
                Some((expire_time, seqnum)) => {
                    if !self.buffer.contains_key(&seqnum) {
                        // Packet has already been ack'd
                    } else if expire_time <= now {
                        // Ready to resend
                        let body = self.buffer.get(&seqnum).unwrap().clone();
                        // Schedule future resend
                        self.timeouts.insert((now + self.resend_timeout, seqnum));
                        return Some(body);
                    } else {
                        // Not expired yet. Re-insert
                        self.timeouts.insert((expire_time, seqnum));
                        return None;
                    }
                }
                None => {
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::collections::HashMap;

    use rand::thread_rng;
    use rand::Rng;

    use crate::wire::command::*;
    use crate::wire::packet::OriginalBody;

    use super::*;

    fn make_inner(index: u32) -> InnerBody {
        // The Hudrm command is only used here because it stores a u32
        // which can be used to verify the packet contents.
        let command = Command::ToClient(ToClientCommand::Hudrm(Box::new(HudrmSpec {
            server_id: index,
        })));
        InnerBody::Original(OriginalBody { command })
    }

    fn recover_index(body: &InnerBody) -> u32 {
        match body {
            InnerBody::Original(body) => match &body.command {
                Command::ToClient(ToClientCommand::Hudrm(spec)) => spec.server_id,
                _ => panic!("Unexpected body"),
            },
            _ => panic!("Unexpected body"),
        }
    }

    /// Ensure that the reliable sender:
    /// 1) Buffers and does not exceed the reliable window size
    /// 2) Retransmits packets that never were never acked, after a timeout.
    /// 3) Continues working after seqnum wraps.
    #[test]
    fn reliable_sender_test() {
        let mut rng = thread_rng();
        let mut r = ReliableSender::new();
        // For each reliable packet, track what happened to it
        // and confirm that it looks correct at the end of the test.
        struct Info {
            sent_time: Vec<Instant>,
            ack_time: Option<Instant>,
        }
        let mut next_index: usize = 0;
        let mut now = Instant::now();
        let mut inflight: HashMap<usize, Info> = HashMap::new();
        let mut sent_but_unacked: BTreeSet<usize> = BTreeSet::new();
        // Simulate activity over time
        // Stops queueing new sends when 1,000,000 packets have been sent
        // Waits for the reliable sender to report nothing to do.
        let mut work_to_do = true;
        while work_to_do {
            work_to_do = false;
            if inflight.len() < 1000000 {
                work_to_do = true;
                // Send 0 to 99 new packets
                for _ in 0..rng.gen_range(0..100) {
                    let inner = make_inner(next_index as u32);
                    r.push(inner);
                    inflight.insert(
                        next_index,
                        Info {
                            sent_time: Vec::new(),
                            ack_time: None,
                        },
                    );
                    next_index += 1;
                }
            }

            // See what it transmits for real
            let mut send_ack_now = Vec::new();
            while let Some(body) = r.pop(now) {
                let recovered_index = recover_index(body.inner()) as usize;
                let info = inflight.get_mut(&recovered_index).unwrap();
                info.sent_time.push(now);
                if info.ack_time.is_none() {
                    sent_but_unacked.insert(recovered_index);
                }

                // Transmission window should never exceed MAX_RELIABLE_WINDOW_SIZE
                if let Some(oldest_unacked_index) = sent_but_unacked.first().map(|v| *v) {
                    assert!(
                        recovered_index >= oldest_unacked_index,
                        "Resending already acknowledged packet"
                    );
                    let spread = recovered_index - oldest_unacked_index;
                    assert!(spread < (MAX_RELIABLE_WINDOW_SIZE as usize));
                }

                // Send acks for 50% of transmitted packets, forcing retries for the others
                // Don't send duplicate acks
                if info.ack_time.is_none() && rng.gen_range(0..2) == 1 {
                    let seqnum = match body {
                        PacketBody::Reliable(rb) => rb.seqnum,
                        PacketBody::Inner(_) => panic!("Unexpected body"),
                    };
                    send_ack_now.push(seqnum);
                    info.ack_time = Some(now);
                    sent_but_unacked.remove(&recovered_index);
                }
            }

            // Send the acks
            for seqnum in send_ack_now.into_iter() {
                r.process_ack(AckBody { seqnum });
            }

            // If we're given a timeout, simulate sleeping until the timeout 50% of the time.
            match r.next_timeout() {
                Some(timeout) => {
                    work_to_do = true;
                    assert!(timeout >= now);
                    if rng.gen_range(0..2) == 1 {
                        now = timeout;
                    } else {
                        now += Duration::from_secs_f32(0.05);
                    }
                }
                None => {
                    now += Duration::from_secs_f32(0.05);
                }
            }
        }

        // Make sure the send intervals are sane
        for (_, info) in inflight.into_iter() {
            // Resend delay should be approximately RESEND_TIMEOUT_START_MS to within 50ms
            for i in 1..info.sent_time.len() {
                let resend_delay = info.sent_time[i] - info.sent_time[i - 1];
                let delta =
                    ((resend_delay.as_millis() as i64) - (RESEND_TIMEOUT_START_MS as i64)).abs();
                assert!(
                    delta < 100,
                    "Unexpected resend interval: {:?}",
                    resend_delay
                );
            }
        }
    }
}
