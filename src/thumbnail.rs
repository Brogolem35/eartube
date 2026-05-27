use std::collections::HashMap;

use iced::{Task, widget::image};
use rustypipe::model::{TrackItem, traits::YtEntity};

use crate::{data::img_cache_dir, gui::Message};

#[derive(Clone, Default)]
pub struct ThumbnailCache {
	inner: HashMap<String, ImageState>,
}

impl ThumbnailCache {
	pub fn get(&self, id: &str) -> image::Handle {
		match self.inner.get(id) {
			Some(ImageState::Loaded(img)) => img.clone(),
			_ => Self::placeholder(),
		}
	}

	pub fn set(&mut self, id: String, img: image::Handle) {
		self.inner.insert(id, ImageState::Loaded(img));
	}

	pub fn fetch(&mut self, src: ThumbnailSource) -> Task<Message> {
		use std::collections::hash_map::Entry;

		let ThumbnailSource { id, url } = src;
		match self.inner.entry(id.clone()) {
			Entry::Vacant(view) => {
				view.insert(ImageState::Loading);
				Task::perform(Self::perform_fetch(id, url), |(id, img)| {
					Message::ImageLoaded { id, img }
				})
			}
			Entry::Occupied(_) => Task::none(),
		}
	}

	async fn perform_fetch(id: String, url: String) -> (String, image::Handle) {
		if let Ok(b) = cacache::read(img_cache_dir(), &id).await {
			(id, image::Handle::from_bytes(b))
		} else if let Ok(b) = reqwest::get(&url).await
			&& let Ok(b) = b.bytes().await
		{
			// Ignored the potential error as it doesn't bother the current execution
			let _ = cacache::write(img_cache_dir(), &id, &b).await;
			(id, image::Handle::from_bytes(b))
		} else {
			(id, Self::placeholder())
		}
	}

	pub fn placeholder() -> image::Handle {
		image::Handle::from_rgba(1, 1, vec![0; 4])
	}
}

#[derive(Clone, Default)]
pub enum ImageState {
	#[default]
	Loading,
	Loaded(image::Handle),
}

#[derive(Clone, Default, Debug)]
pub struct ThumbnailSource {
	id: String,
	url: String,
}

impl ThumbnailSource {
	pub fn new(track: &TrackItem) -> Self {
		let urls = track.cover.iter();
		let chosen = match urls
			.clone()
			.filter(|t| t.height >= 80)
			.min_by_key(|t| t.height)
		{
			Some(u) => u,
			None => urls.max_by_key(|t| t.height).expect("Empty URL list"),
		};

		let url = chosen.url.clone();
		let id = track.id().to_string();
		Self { id, url }
	}
}
