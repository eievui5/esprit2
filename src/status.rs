use crate::prelude::*;
use mlua::LuaSerdeExt;
use std::cell::Cell;
use tracing::{error, warn};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Duration {
	Rest,
	Turn,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Debuff {
	#[serde(skip)]
	magnitude: u32,
	on_debuff: script::MaybeInline,

	#[serde(skip)]
	cache: Cell<Option<(u32, character::Stats)>>,
}

impl Debuff {
	fn get_script(&self) -> Result<character::Stats> {
		thread_local! { static LUA: mlua::Lua = mlua::Lua::new() }
		LUA.with(|lua| {
			lua.globals().set("magnitude", self.magnitude)?;
			let stats = lua.from_value(lua.load(self.on_debuff.contents()).eval()?)?;
			Ok(stats)
		})
	}

	pub fn get(&self) -> Option<character::Stats> {
		if let Some((last_magnitude, cache)) = self.cache.get() {
			if self.magnitude == last_magnitude {
				return Some(cache);
			}
		}
		let stats = self
			.get_script()
			.map_err(|msg| error!("failed to calculate debuff: {msg}"))
			.unwrap_or_default();
		self.cache.set(Some((self.magnitude, stats)));
		Some(stats)
	}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Effect {
	StaticDebuff(character::Stats),
	Debuff(Debuff),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Status {
	pub name: String,
	pub icon: String,
	pub duration: Duration,
	pub effect: Effect,
}

impl Status {
	pub fn add_magnitude(&mut self, amount: u32) {
		match &mut self.effect {
			Effect::Debuff(Debuff { magnitude, .. }) => {
				*magnitude = magnitude.saturating_add(amount)
			}
			Effect::StaticDebuff(_) => {
				warn!(
					"attempted to increase the magnitude of \"{}\" but it had none",
					self.name
				);
			}
		}
	}

	pub fn on_debuff(&self) -> Option<character::Stats> {
		match &self.effect {
			Effect::Debuff(debuff) => debuff.get(),
			Effect::StaticDebuff(debuff) => Some(*debuff),
		}
	}
}

impl mlua::UserData for Status {}
