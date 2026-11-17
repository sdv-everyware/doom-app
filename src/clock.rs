// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The game's clock: milliseconds that stand still while the game pauses (the panel's dialog is
//! closed). The engine runs as many tics as its clock moved since the last frame
//! (d_loop.c TryRunTics), so a clock that ran on through a pause would play the minutes of the
//! pause at once when the dialog opens again.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    start: Instant,
    /// The pauses that ended.
    paused: Duration,
    /// The pause now, since when.
    since: Option<Instant>,
}

impl Clock {
    /// A clock that starts at `now`, running.
    pub fn new(now: Instant) -> Self {
        Clock { start: now, paused: Duration::ZERO, since: None }
    }

    /// The milliseconds the game ran until `now`, from 1: the engine takes its first reading
    /// for its start, and a 0 for none yet (i_timer.c basetime).
    pub fn ms(&self, now: Instant) -> u32 {
        let pause_now = self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s));
        ms_of(now.saturating_duration_since(self.start).saturating_sub(self.paused + pause_now))
    }

    /// Stops the clock at `now`; again while stopped changes nothing.
    pub fn pause(&mut self, now: Instant) {
        if self.since.is_none() {
            self.since = Some(now);
        }
    }

    /// Runs the clock on from where it stopped.
    pub fn resume(&mut self, now: Instant) {
        if let Some(s) = self.since.take() {
            self.paused += now.saturating_duration_since(s);
        }
    }
}

/// The engine's milliseconds after `ran` of play: a 32-bit count that wraps after 49.7 days, as
/// the engine's own arithmetic does (i_timer.c), instead of an overflow that ends the process.
fn ms_of(ran: Duration) -> u32 {
    (ran.as_millis() as u32).wrapping_add(1)
}

/// How long to wait before the next try to reach the panel: 250 ms, twice as long after each
/// failure, at most 5 s; back to the start once it was reached.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    next: Duration,
}

const BACKOFF_FIRST: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(5);

impl Default for Backoff {
    fn default() -> Self {
        Backoff { next: BACKOFF_FIRST }
    }
}

impl Backoff {
    /// The wait before the next try.
    pub fn wait(&mut self) -> Duration {
        let w = self.next;
        self.next = (self.next * 2).min(BACKOFF_MAX);
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_wraps_instead_of_overflowing() {
        assert_eq!(ms_of(Duration::ZERO), 1);
        assert_eq!(ms_of(Duration::from_millis(u64::from(u32::MAX) - 1)), u32::MAX);
        assert_eq!(ms_of(Duration::from_millis(u64::from(u32::MAX))), 0);
        assert_eq!(ms_of(Duration::from_millis(u64::from(u32::MAX) + 1)), 1);
    }

    #[test]
    fn the_tries_slow_down_to_five_seconds() {
        let mut b = Backoff::default();
        let waits: Vec<u64> = (0..7).map(|_| b.wait().as_millis() as u64).collect();
        assert_eq!(waits, [250, 500, 1000, 2000, 4000, 5000, 5000]);
    }

    #[test]
    fn a_pause_takes_no_time() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut clock = Clock::new(t0);
        assert_eq!(clock.ms(t0), 1);
        assert_eq!(clock.ms(at(100)), 101);
        clock.pause(at(100));
        assert_eq!(clock.ms(at(60_000)), 101, "a minute closed is no minute played");
        clock.pause(at(70_000));
        clock.resume(at(80_000));
        assert_eq!(clock.ms(at(80_000)), 101);
        assert_eq!(clock.ms(at(80_035)), 136);
        clock.resume(at(90_000));
        assert_eq!(clock.ms(at(90_000)), 10_101, "a resume while running changes nothing");
    }
}
