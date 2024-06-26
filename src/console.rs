use crate::prelude::*;
use mlua::LuaSerdeExt;
use paste::paste;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::rect::Rect;
use sdl2::render::TextureQuery;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc};

const MINIMUM_NAMEPLATE_WIDTH: u32 = 100;

#[derive(Debug)]
pub struct Console {
	pub handle: Handle,
	message_reciever: mpsc::Receiver<Message>,
	history: Vec<Message>,
	in_progress: VecDeque<usize>,
}

impl std::ops::Deref for Console {
	type Target = Handle;

	fn deref(&self) -> &Self::Target {
		&self.handle
	}
}

#[derive(Clone, Debug)]
pub struct Handle {
	message_sender: mpsc::Sender<Message>,
	pub colors: Colors,
}

#[derive(Clone, Debug)]
pub enum MessagePrinter {
	Console(Color),
	Dialogue { speaker: Arc<str>, progress: f64 },
	Combat(combat::Log),
}

#[derive(Clone, Debug)]
pub struct Message {
	text: String,
	printer: MessagePrinter,
}

macro_rules! console_colored_print {
	(normal) => {
		pub fn print(&self, text: String) {
			let _ = self.message_sender.send(Message {
				text,
				printer: MessagePrinter::Console(self.colors.normal),
			});
		}
	};

	($which:ident) => {
		paste! {
			pub fn [<print_ $which>](&self, text: String) {
				let _ = self.message_sender.send(Message {
					text,
					printer: MessagePrinter::Console(self.colors.$which),
				});
			}
		}
	};
}

macro_rules! handle_colored_print {
	(normal, $methods:ident) => {
		$methods.add_method("print", |_, this, value: String| {
			this.message_sender
				.send(Message {
					text: value,
					printer: MessagePrinter::Console(this.colors.normal),
				})
				.map_err(mlua::Error::external)
		});
	};

	($which:ident, $methods:ident) => {
		paste! {
			$methods.add_method(concat!("print_", stringify!($which)), |_, this, value: String| {
				this.message_sender
					.send(Message {
						text: value,
						printer: MessagePrinter::Console(this.colors.$which),
					})
					.map_err(mlua::Error::external)
			});
		}
	};
}

macro_rules! impl_console {
	(
		$(impl $impl_colors:ident: $impl_value:expr,)+
		$(let $colors:ident: $value:expr,)+
	) => {
		#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
		pub struct Colors {
			$(pub $colors: Color,)*
			$(pub $impl_colors: Color,)*
		}

		impl Default for Colors {
			fn default() -> Self {
				Self {
					$($impl_colors: $impl_value,)*
					$($colors: $value,)*
				}
			}
		}

		impl Handle {
			$(console_colored_print! { $impl_colors } )*

			pub fn print_colored(&self, text: String, color: Color) {
				let _ = self.message_sender.send(Message {
					text,
					printer: MessagePrinter::Console(color),
				});
			}

			pub fn say(&self, speaker: Arc<str>, text: String) {
				let _ = self.message_sender.send(Message {
					text,
					printer: MessagePrinter::Dialogue {
						speaker,
						progress: 0.0,
					},
				});
			}

			pub fn combat_log(&self, text: String, log: combat::Log) {
				let  _ = self.message_sender.send(Message {
					text,
					printer: MessagePrinter::Combat(log),
				});
			}
		}

		impl mlua::UserData for Handle {
			fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
				$(handle_colored_print! { $impl_colors, methods } )*
				methods.add_method("combat_log", |lua, this, (text, log): (String, mlua::Value)| {
					let log = lua.from_value(log)?;
					this.message_sender
						.send(Message {
							text,
							printer: MessagePrinter::Combat(log),
						})
						.map_err(mlua::Error::external)
				});

			}
		}
	};
}

impl_console! {
	impl normal: (255, 255, 255, 255),
	impl system: (100, 100, 100, 255),
	impl unimportant: (100, 100, 100, 255),
	impl defeat: (255, 128, 128, 255),
	impl danger: (255, 0, 0, 255),
	impl important: (255, 255, 0, 255),
	impl special: (0, 255, 0, 255),
	let combat: (255, 255, 128, 255),
}

impl Default for Console {
	fn default() -> Self {
		let (message_sender, message_reciever) = mpsc::channel();
		Self {
			message_reciever,
			history: Vec::new(),
			in_progress: VecDeque::new(),
			handle: Handle {
				message_sender,
				colors: Colors::default(),
			},
		}
	}
}

impl Console {
	pub fn new(colors: console::Colors) -> Self {
		let mut result = Self::default();
		result.handle.colors = colors;
		result
	}
}

impl Console {
	pub fn update(&mut self, delta: f64) {
		for message in self.message_reciever.try_iter() {
			let is_dialogue = matches!(message.printer, MessagePrinter::Dialogue { .. });
			self.history.push(message);
			if is_dialogue {
				self.in_progress.push_back(self.history.len() - 1);
			}
		}

		let delta_progress = delta / 0.1;

		for i in &self.in_progress {
			let i = *i;
			let max_length = self.history[i].text.len() as f64;
			if let MessagePrinter::Dialogue {
				speaker: _,
				progress,
			} = &mut self.history[i].printer
			{
				let new_progress = *progress + delta_progress;
				if new_progress < max_length {
					*progress = new_progress;
				}
			}
		}

		while self.in_progress.front().is_some_and(|x| {
			if let MessagePrinter::Dialogue {
				speaker: _,
				progress,
			} = &self.history[*x].printer
			{
				self.history[*x].text.len() == (*progress as usize)
			} else {
				true
			}
		}) {
			self.in_progress.pop_front();
		}
	}

	#[allow(clippy::unwrap_used, reason = "SDL")]
	pub fn draw(&self, gui: &mut gui::Context) {
		let canvas = &mut gui.canvas;
		let rect = Rect::new(
			gui.x,
			gui.y,
			(gui.rect.right() - gui.x) as u32,
			(gui.rect.bottom() - gui.y) as u32,
		);
		let font_texture_creator = canvas.texture_creator();
		canvas.set_clip_rect(rect);

		let mut cursor = rect.y + (rect.height() as i32);

		let text = |message, color: Color| {
			let texture = gui
				.typography
				.normal
				.render(message)
				.blended(color)
				.unwrap()
				.as_texture(&font_texture_creator)
				.unwrap();
			let TextureQuery { width, height, .. } = texture.query();
			(texture, width, height)
		};
		for message in self.history.iter().rev() {
			match &message.printer {
				MessagePrinter::Console(color) => {
					let (font_texture, width, height) = text(&message.text, *color);
					cursor -= height as i32;
					canvas
						.copy(
							&font_texture,
							None,
							Rect::new(rect.x, cursor, width, height),
						)
						.unwrap();
				}
				MessagePrinter::Dialogue { speaker, progress } => {
					let (font_texture, text_width, height) = text(speaker, (0, 0, 0, 255));
					let width = text_width.max(MINIMUM_NAMEPLATE_WIDTH);
					let margin = ((width - text_width) / 2) as i32;
					canvas
						.rounded_box(
							rect.x as i16,
							cursor as i16,
							(rect.x + (width as i32)) as i16,
							(cursor - (height as i32) + 2) as i16,
							5,
							self.colors.normal,
						)
						.unwrap();
					cursor -= height as i32;
					canvas
						.copy(
							&font_texture,
							None,
							Rect::new(rect.x + margin, cursor, text_width, height),
						)
						.unwrap();

					// Save width of nameplate.
					let last_width = width as i32;

					let shown_characters = message.text.len().min((*progress as usize) + 1);
					let (font_texture, width, height) =
						text(&message.text[0..shown_characters], self.colors.normal);
					canvas
						.copy(
							&font_texture,
							None,
							Rect::new(rect.x + last_width + 10, cursor, width, height),
						)
						.unwrap();
				}
				MessagePrinter::Combat(log) => {
					let color = if log.is_weak() {
						self.colors.unimportant
					} else {
						self.colors.normal
					};
					let (texture, width, height) = text(&message.text, color);
					cursor -= height as i32;
					canvas
						.copy(&texture, None, Rect::new(rect.x, cursor, width, height))
						.unwrap();
					let last_width = width as i32;
					let info = format!("({log})");
					let texture = gui
						.typography
						.annotation
						.render(&info)
						.blended(self.colors.combat)
						.unwrap()
						.as_texture(&font_texture_creator)
						.unwrap();
					let TextureQuery { width, height, .. } = texture.query();
					canvas
						.copy(
							&texture,
							None,
							Rect::new(rect.x + last_width + 10, cursor, width, height),
						)
						.unwrap();
				}
			}

			if cursor < rect.y {
				break;
			}
		}

		canvas.set_clip_rect(None);
	}
}
