use crate::prelude::*;
use esprit2::prelude::*;
use sdl2::rect::Rect;
use std::{net::ToSocketAddrs, process::exit};

pub struct State<'texture> {
	pub server: ServerHandle,

	pub resources: resource::Manager,
	pub console: Console,
	pub soul_jar: gui::widget::SoulJar<'texture>,
	pub cloudy_wave: draw::CloudyWave,
	pub pamphlet: gui::widget::Pamphlet,
	pub chase_point: Option<select::Point>,
}

impl<'texture> State<'texture> {
	pub fn new<'lua>(
		address: impl ToSocketAddrs,
		lua: &'lua mlua::Lua,
		textures: &'texture texture::Manager,
	) -> Result<Self> {
		// Create a console.
		// An internal server will send messages to it using a console::Handle.
		// An external server will send messages to it over TCP. (local messages generated by the world cache are discarded)
		let console = Console::default();

		// Create an internal server instance
		let server = ServerHandle::new(address);
		let resources = resource::Manager::open(options::resource_directory())?;

		let mut soul_jar = gui::widget::SoulJar::new(textures).unwrap();
		// This disperses the souls enough to cause them to fly in from the sides
		// the same effect can be seen if a computer is put to sleep and then woken up.
		soul_jar.tick(5.0);
		let cloudy_wave = draw::CloudyWave::default();
		let pamphlet = gui::widget::Pamphlet::new();

		// TODO: Make this part of input::Mode::Select;
		let chase_point = None;

		lua.globals()
			.set("Console", console::LuaHandle(console_impl::Dummy))
			.unwrap();
		lua.globals()
			.set("Status", resources.statuses_handle())
			.unwrap();
		lua.globals()
			.set("Heuristic", consider::HeuristicConstructor)
			.unwrap();
		lua.globals().set("Log", combat::LogConstructor).unwrap();
		lua.globals()
			.set("Input", input::RequestConstructor)
			.unwrap();

		Ok(Self {
			server,

			resources,
			console,
			soul_jar,
			cloudy_wave,
			pamphlet,
			chase_point,
		})
	}

	pub fn event<'lua>(
		&mut self,
		input_mode: input::Mode<'lua>,
		event: sdl2::event::Event,
		scripts: &resource::Scripts<'lua>,
		options: &Options,
	) -> input::Mode<'lua> {
		let sdl2::event::Event::KeyDown {
			keycode: Some(keycode),
			..
		} = event
		else {
			return input_mode;
		};
		if !self
			.server
			.world()
			.next_character()
			.borrow()
			.player_controlled
		{
			return input_mode;
		}
		match input::controllable_character(
			keycode,
			self.server.world(),
			&self.console.handle,
			&self.resources,
			scripts,
			input_mode,
			options,
		) {
			Ok((mode, response)) => match response {
				Some(input::Response::Select(point)) => {
					self.chase_point = Some(point);
					mode
				}
				Some(input::Response::Act(action)) => {
					self.server
						.send_action(&self.resources, scripts, action)
						.unwrap();
					mode
				}

				Some(input::Response::Partial(partial, request)) => match request {
					input::Request::Cursor {
						x,
						y,
						range,
						radius,
					} => input::Mode::Cursor(input::Cursor {
						origin: (x, y),
						position: (x, y),
						range,
						radius,
						state: input::CursorState::default(),
						callback: partial,
					}),
					input::Request::Prompt { message } => input::Mode::Prompt(input::Prompt {
						message,
						callback: partial,
					}),
					input::Request::Direction { message } => {
						input::Mode::DirectionPrompt(input::DirectionPrompt {
							message,
							callback: partial,
						})
					}
				},
				None => mode,
			},
			Err(msg) => {
				error!("world input processing returned an error: {msg}");
				input::Mode::Normal
			}
		}
	}

	pub fn tick<'lua>(
		&mut self,
		delta: f64,
		input_mode: &mut input::Mode<'lua>,
		scripts: &resource::Scripts<'lua>,
	) {
		let next_character = self.server.world().next_character().clone();
		if next_character.borrow().player_controlled {
			if let Some(point) = &self.chase_point {
				match point {
					select::Point::Character(character) => {
						let (x, y) = {
							let c = character.borrow();
							(c.x, c.y)
						};
						// Give a safe range of 2 tiles if the target is an enemy.
						let distance =
							if next_character.borrow().alliance != character.borrow().alliance {
								2
							} else {
								1
							};
						if (next_character.borrow().x - x).abs() <= distance
							&& (next_character.borrow().y - y).abs() <= distance
						{
							self.chase_point = None;
						} else {
							self.server
								.send_action(
									&self.resources,
									scripts,
									character::Action::Move(x, y),
								)
								.unwrap();
						}
					}
					select::Point::Exit(x, y) => {
						if next_character.borrow().x == *x && next_character.borrow().y == *y {
							self.chase_point = None;
						} else {
							self.server
								.send_action(
									&self.resources,
									scripts,
									character::Action::Move(*x, *y),
								)
								.unwrap();
						}
					}
				}
			}
		}

		if let Err(msg) = self.server.tick(&mut self.console) {
			error!("server tick failed: {msg}");
			exit(1);
		}

		for i in &mut self.pamphlet.party_member_clouds {
			i.cloud.tick(delta);
			i.cloud_trail.tick(delta / 4.0);
		}
		self.console.update(delta);
		self.soul_jar.tick(delta as f32);
		self.cloudy_wave.tick(delta);
		if let input::Mode::Cursor(input::Cursor { state, .. }) = input_mode {
			state.float.increment(delta * 0.75);
		}
	}

	pub fn draw<'lua>(
		&self,
		input_mode: &input::Mode<'lua>,
		ctx: &mut gui::Context,
		textures: &'texture texture::Manager,
		options: &Options,
	) {
		// Render World
		let width = 480;
		let height = 320;
		let mut camera = draw::Camera::default();
		camera.update_size(width, height);
		let focused_character = &self
			.server
			.world()
			.characters
			.iter()
			.find(|x| x.borrow().player_controlled)
			.unwrap();
		if let input::Mode::Cursor(input::Cursor { position, .. }) = &input_mode {
			camera.focus_character_with_cursor(&focused_character.borrow(), *position);
		} else {
			camera.focus_character(&focused_character.borrow());
		}

		let texture_creator = ctx.canvas.texture_creator();
		let mut world_texture = texture_creator
			.create_texture_target(texture_creator.default_pixel_format(), width, height)
			.unwrap();

		ctx.canvas
			.with_texture_canvas(&mut world_texture, |canvas| {
				canvas.set_draw_color((20, 20, 20));
				canvas.clear();
				draw::tilemap(canvas, self.server.world(), &camera);
				draw::characters(canvas, self.server.world(), textures, &camera);
				draw::cursor(canvas, input_mode, textures, &camera);
			})
			.unwrap();

		ctx.canvas
			.copy(
				&world_texture,
				None,
				Rect::new(
					(ctx.rect.width() as i32
						- options.ui.pamphlet_width as i32
						- width as i32 * options.board.scale as i32)
						/ 2,
					(ctx.rect.height() as i32
						- options.ui.console_height as i32
						- height as i32 * options.board.scale as i32)
						/ 2,
					width * options.board.scale,
					height * options.board.scale,
				),
			)
			.unwrap();

		// Render User Interface
		ctx.canvas.set_viewport(None);

		let mut menu = ctx.view(
			0,
			(ctx.rect.height() - options.ui.console_height) as i32,
			ctx.rect.width() - options.ui.pamphlet_width,
			options.ui.console_height,
		);
		gui::widget::menu(
			&mut menu,
			options,
			input_mode,
			self.server.world(),
			&self.console,
			&self.resources,
			textures,
		);

		// Draw pamphlet
		let mut pamphlet_ctx = ctx.view(
			(ctx.rect.width() - options.ui.pamphlet_width) as i32,
			0,
			options.ui.pamphlet_width,
			ctx.rect.height(),
		);

		self.cloudy_wave.draw(
			pamphlet_ctx.canvas,
			pamphlet_ctx.rect,
			20,
			(0x08, 0x0f, 0x25).into(),
		);

		self.pamphlet.draw(
			&mut pamphlet_ctx,
			self.server.world(),
			textures,
			&self.soul_jar,
		);
	}
}
