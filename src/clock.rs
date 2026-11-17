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
        let ran = now.saturating_duration_since(self.start).saturating_sub(self.paused + pause_now);
        1 + ran.as_millis() as u32
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

#[cfg(test)]
mod tests {
    use super::*;

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
