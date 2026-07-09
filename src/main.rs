mod audio_cache;
mod data;
mod gui;
mod icons;
mod playback;
mod player;
mod screen;
mod thumbnail;
mod yt;

use std::{thread, time::Duration};

use parking_lot::deadlock;

use crate::gui::iced_main;

fn main() -> iced::Result {
	start_deadlock_check();
	iced_main()
}

fn start_deadlock_check() {
	// From: https://docs.rs/parking_lot/latest/parking_lot/deadlock/index.html
	// Create a background thread which checks for deadlocks every 10s
	thread::spawn(move || {
		loop {
			thread::sleep(Duration::from_secs(10));
			let deadlocks = deadlock::check_deadlock();
			if deadlocks.is_empty() {
				continue;
			}

			eprintln!("{} deadlocks detected", deadlocks.len());
			for (i, threads) in deadlocks.iter().enumerate() {
				eprintln!("Deadlock #{}", i);
				for t in threads {
					eprintln!("Thread Id {:#?}", t.thread_id());
					eprintln!("{:#?}", t.backtrace());
				}
			}
		}
	});
}
