use crate::prelude::*;
use nouns::StrExt;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Used for debugging.
fn force_affinity(_lua: &mlua::Lua, this: &Ref, index: u32) -> mlua::Result<()> {
	this.borrow_mut().sheet.skillset = match index {
		0 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Positive,
			minor: None,
		},
		1 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Positive,
			minor: Some(spell::Harmony::Chaos),
		},
		2 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Positive,
			minor: Some(spell::Harmony::Order),
		},
		3 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Negative,
			minor: None,
		},
		4 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Negative,
			minor: Some(spell::Harmony::Chaos),
		},
		5 => spell::Skillset::EnergyMajor {
			major: spell::Energy::Negative,
			minor: Some(spell::Harmony::Order),
		},
		6 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Chaos,
			minor: None,
		},
		7 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Chaos,
			minor: Some(spell::Energy::Positive),
		},
		8 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Chaos,
			minor: Some(spell::Energy::Negative),
		},
		9 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Order,
			minor: None,
		},
		10 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Order,
			minor: Some(spell::Energy::Positive),
		},
		11 => spell::Skillset::HarmonyMajor {
			major: spell::Harmony::Order,
			minor: Some(spell::Energy::Negative),
		},
		_ => {
			return Err(mlua::Error::runtime("invalid affinity index"));
		}
	};
	Ok(())
}

/// Initializes an effect with the given magnitude, or adds the magnitude to the effect if it already exists.
pub fn inflict(
	lua: &mlua::Lua,
	this: &Ref,
	(key, magnitude): (String, Option<u32>),
) -> mlua::Result<()> {
	let statuses = lua
		.globals()
		.get::<&str, resource::Handle<Status>>("Status")?;
	let Some(status) = statuses.0.get(key.as_str()).cloned() else {
		return Err(mlua::Error::external(resource::Error::NotFound(key)));
	};
	let mut entry = this.borrow_mut();
	let entry = entry
		.statuses
		.entry(key.into_boxed_str())
		.or_insert_with(|| status);
	if let Some(magnitude) = magnitude {
		entry.add_magnitude(magnitude);
	}
	Ok(())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, mlua::FromLua)]
pub struct Ref(Rc<RefCell<character::Piece>>);

impl Ref {
	pub fn new(character: character::Piece) -> Self {
		Self(Rc::new(RefCell::new(character)))
	}
}

impl std::ops::Deref for Ref {
	type Target = Rc<RefCell<character::Piece>>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl std::ops::DerefMut for Ref {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl mlua::UserData for Ref {
	fn add_fields<'lua, F: mlua::prelude::LuaUserDataFields<'lua, Self>>(fields: &mut F) {
		macro_rules! get {
			($field:ident) => {
				fields.add_field_method_get(stringify!($field), |_, this| Ok(this.borrow().$field.clone()));
			};
			($($field:ident),+$(,)?) => {
				$( get! { $field } )+
			}
		}
		macro_rules! set {
			($field:ident) => {
				fields.add_field_method_set(stringify!($field), |_, this, value| {
					this.borrow_mut().$field = value;
					Ok(())
				});
			};
			($($field:ident),+$(,)?) => {
				$( set! { $field } )+
			}
		}
		fields.add_field_method_get("sheet", |_, this| Ok(this.borrow().sheet.clone()));
		fields.add_field_method_get("stats", |_, this| Ok(this.borrow().stats()));
		fields.add_field_method_get("alliance", |_, this| Ok(this.borrow().alliance as u32));
		get!(hp, sp, x, y);
		set!(hp, sp, x, y);
	}

	fn add_methods<'lua, M: mlua::prelude::LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_method("replace_nouns", |_, this, s: String| {
			Ok(s.replace_nouns(&this.borrow().sheet.nouns))
		});
		methods.add_method(
			"replace_prefixed_nouns",
			|_, this, (prefix, string): (String, String)| {
				Ok(string.replace_prefixed_nouns(&this.borrow().sheet.nouns, &prefix))
			},
		);
		methods.add_method("force_level", |_, this, ()| {
			let level = &mut this.borrow_mut().sheet.level;
			*level = level.saturating_add(1);
			Ok(())
		});
		// TODO: Make these functions into Rust methods of Piece.
		methods.add_method("force_affinity", force_affinity);
		methods.add_method("inflict", inflict);
	}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Piece {
	pub sheet: Sheet,

	pub hp: i32,
	pub sp: i32,

	pub statuses: HashMap<Box<str>, Status>,
	pub attacks: Vec<Rc<Attack>>,
	pub spells: Vec<Rc<Spell>>,

	pub x: i32,
	pub y: i32,
	pub action_delay: Aut,
	pub player_controlled: bool,
	pub alliance: Alliance,

	// Temporary associated storage
	// TODO: Does this even need to be a field?
	#[serde(skip)]
	pub next_action: Option<Action>,
}

impl expression::Variables for Piece {
	fn get(&self, s: &str) -> Result<expression::Integer, expression::Error> {
		match s {
			"hp" => Ok(self.hp as expression::Integer),
			"sp" => Ok(self.sp as expression::Integer),
			_ => self.sheet.get(s),
		}
	}
}

impl Piece {
	pub fn new(sheet: Sheet, resources: &resource::Manager) -> Result<Self> {
		let stats = sheet.stats();
		let hp = stats.heart as i32;
		let sp = stats.soul as i32;
		let attacks = sheet
			.attacks
			.iter()
			.map(|x| resources.get_attack(x).cloned())
			.collect::<Result<_>>()?;
		let spells = sheet
			.spells
			.iter()
			.map(|x| resources.get_spell(x).cloned())
			.collect::<Result<_>>()?;

		Ok(Self {
			sheet,
			hp,
			sp,
			statuses: HashMap::new(),
			attacks,
			spells,
			x: 0,
			y: 0,
			next_action: None,
			action_delay: 0,
			player_controlled: false,
			alliance: Alliance::default(),
		})
	}

	pub fn new_turn(&mut self) {
		// Remove any status effects with the duration of one turn.
		self.statuses
			.retain(|_, status| !matches!(status.duration, status::Duration::Turn));
	}

	pub fn rest(&mut self) {
		let stats = self.stats();
		self.restore_hp(stats.heart / 2);
		self.restore_sp(stats.soul);
		// Remove any status effects lasting until the next rest.
		self.statuses
			.retain(|_, status| !matches!(status.duration, status::Duration::Rest));
	}

	pub fn restore_hp(&mut self, amount: u32) {
		self.hp = i32::min(self.hp + amount as i32, self.stats().heart as i32);
	}

	pub fn restore_sp(&mut self, amount: u32) {
		self.sp = i32::min(self.sp + amount as i32, self.stats().soul as i32);
	}
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StatOutcomes {
	pub stats: Stats,
	pub buffs: Stats,
	pub debuffs: Stats,
}

impl Piece {
	pub fn stats(&self) -> Stats {
		self.stat_outcomes().stats
	}

	pub fn stat_outcomes(&self) -> StatOutcomes {
		let buffs = Stats::default();
		let mut debuffs = Stats::default();

		for debuff in self.statuses.values().filter_map(|x| x.on_debuff()) {
			debuffs = debuffs + debuff;
		}

		let mut stats = self.sheet.stats();
		stats.heart = stats.heart.saturating_sub(debuffs.heart) + buffs.heart;
		stats.soul = stats.soul.saturating_sub(debuffs.soul) + buffs.soul;
		stats.power = stats.power.saturating_sub(debuffs.power) + buffs.power;
		stats.defense = stats.defense.saturating_sub(debuffs.defense) + buffs.defense;
		stats.magic = stats.magic.saturating_sub(debuffs.magic) + buffs.magic;
		stats.resistance = stats.resistance.saturating_sub(debuffs.resistance) + buffs.resistance;

		StatOutcomes {
			stats,
			buffs,
			debuffs,
		}
	}
}

/// Anything a character piece can "do".
///
/// This is the only way that character logic or player input should communicate with pieces.
/// The information here should be enough to perform the action, but in the event it isn't
/// (from an incomplete player input), an `ActionRequest` will be yielded to fill in the missing information.
#[derive(Clone, Debug)]
pub enum Action {
	Wait(Aut),
	Move(i32, i32),
	Attack(Rc<Attack>, Option<mlua::OwnedTable>),
	Cast(Rc<Spell>, Option<mlua::OwnedTable>),
}

#[derive(Copy, PartialEq, Eq, Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(u32)]
pub enum Alliance {
	Friendly,
	#[default]
	Enemy,
}

mod sheet {
	use super::*;

	fn stats(_lua: &mlua::Lua, this: &mut Sheet, _: ()) -> mlua::Result<Stats> {
		Ok(this.stats())
	}

	fn growth_bonuses() -> Stats {
		use rand::seq::SliceRandom;
		const BONUS_COUNT: usize = 10;

		let mut bonuses = Stats::default();
		let mut stats = [
			&mut bonuses.heart,
			&mut bonuses.soul,
			&mut bonuses.power,
			&mut bonuses.defense,
			&mut bonuses.magic,
			&mut bonuses.resistance,
		];
		let mut rng = rand::thread_rng();

		for _ in 0..BONUS_COUNT {
			let stat = stats
				.choose_mut(&mut rng)
				.expect("stats should not be empty");
			// Prefer skipping stats that are already 0
			if **stat == 0 {
				**stats
					.choose_mut(&mut rng)
					.expect("stats should not be empty") += 1;
			} else {
				**stat += 1;
			}
		}

		bonuses
	}

	#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, alua::UserData)]
	#[alua(method = stats)]
	pub struct Sheet {
		pub icon: String,
		/// Note that this includes the character's name.
		#[alua(get)]
		pub nouns: Nouns,

		#[alua(get)]
		pub level: u32,
		#[alua(get)]
		#[serde(default)] // There's no reason for most sheets to care about this.
		pub experience: u32,

		#[alua(get)]
		pub bases: Stats,
		#[alua(get)]
		pub growths: Stats,
		#[serde(default = "growth_bonuses")]
		pub growth_bonuses: Stats,

		pub skillset: spell::Skillset,
		#[alua(get)]
		pub speed: Aut,

		#[alua(get)]
		pub attacks: Vec<String>,
		#[alua(get)]
		pub spells: Vec<String>,

		/// Script to decide on an action from a list of considerations
		pub on_consider: Script,
	}
}

pub use sheet::Sheet;

impl Sheet {
	pub fn stats(&self) -> Stats {
		const BONUS_WEIGHTS: Stats = Stats {
			heart: 20,
			soul: 20,
			power: 10,
			defense: 10,
			magic: 10,
			resistance: 10,
		};

		self.bases + (self.growths + self.growth_bonuses * BONUS_WEIGHTS) * self.level / 100
	}
}

impl expression::Variables for Sheet {
	fn get(&self, s: &str) -> Result<expression::Integer, expression::Error> {
		match s {
			"level" => Ok(self.level as expression::Integer),
			"speed" => Ok(self.speed as expression::Integer),
			_ => self.stats().get(s),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize, alua::UserData)]
pub struct Stats {
	/// Health, or HP; Heart Points
	#[serde(default)]
	#[alua(get)]
	pub heart: u32,
	/// Magic, or SP; Soul Points
	#[serde(default)]
	#[alua(get)]
	pub soul: u32,
	/// Bonus damage applied to physical attacks.
	#[serde(default)]
	#[alua(get)]
	pub power: u32,
	/// Damage reduction when recieving physical attacks.
	#[serde(default)]
	#[alua(get)]
	pub defense: u32,
	/// Bonus damage applied to magical attacks.
	#[serde(default)]
	#[alua(get)]
	pub magic: u32,
	/// Damage reduction when recieving magical attacks.
	/// Also makes harmful spells more likely to fail.
	#[serde(default)]
	#[alua(get)]
	pub resistance: u32,
}

impl std::ops::Add for Stats {
	type Output = Stats;

	fn add(self, rhs: Self) -> Self {
		Stats {
			heart: self.heart + rhs.heart,
			soul: self.soul + rhs.soul,
			power: self.power + rhs.power,
			defense: self.defense + rhs.defense,
			magic: self.magic + rhs.magic,
			resistance: self.resistance + rhs.resistance,
		}
	}
}

impl std::ops::Sub for Stats {
	type Output = Stats;

	fn sub(self, rhs: Self) -> Self {
		Stats {
			heart: self.heart - rhs.heart,
			soul: self.soul - rhs.soul,
			power: self.power - rhs.power,
			defense: self.defense - rhs.defense,
			magic: self.magic - rhs.magic,
			resistance: self.resistance - rhs.resistance,
		}
	}
}

impl std::ops::Mul<u32> for Stats {
	type Output = Stats;

	fn mul(self, rhs: u32) -> Self {
		Stats {
			heart: self.heart * rhs,
			soul: self.soul * rhs,
			power: self.power * rhs,
			defense: self.defense * rhs,
			magic: self.magic * rhs,
			resistance: self.resistance * rhs,
		}
	}
}

impl std::ops::Mul for Stats {
	type Output = Stats;

	fn mul(self, rhs: Self) -> Self {
		Stats {
			heart: self.heart * rhs.heart,
			soul: self.soul * rhs.soul,
			power: self.power * rhs.power,
			defense: self.defense * rhs.defense,
			magic: self.magic * rhs.magic,
			resistance: self.resistance * rhs.resistance,
		}
	}
}

impl std::ops::Div<u32> for Stats {
	type Output = Stats;

	fn div(self, rhs: u32) -> Self {
		Stats {
			heart: self.heart / rhs,
			soul: self.soul / rhs,
			power: self.power / rhs,
			defense: self.defense / rhs,
			magic: self.magic / rhs,
			resistance: self.resistance / rhs,
		}
	}
}

impl expression::Variables for Stats {
	fn get(&self, s: &str) -> Result<expression::Integer, expression::Error> {
		match s {
			"heart" => Ok(self.heart as expression::Integer),
			"soul" => Ok(self.soul as expression::Integer),
			"power" => Ok(self.power as expression::Integer),
			"defense" => Ok(self.defense as expression::Integer),
			"magic" => Ok(self.magic as expression::Integer),
			"resistance" => Ok(self.resistance as expression::Integer),
			_ => Err(expression::Error::MissingVariable(s.into())),
		}
	}
}

const HEART_COLOR: Color = (96, 67, 18, 255);
const SOUL_COLOR: Color = (128, 128, 128, 255);
const POWER_COLOR: Color = (255, 11, 64, 255);
const DEFENSE_COLOR: Color = (222, 120, 64, 255);
const MAGIC_COLOR: Color = (59, 115, 255, 255);
const RESISTANCE_COLOR: Color = (222, 64, 255, 255);

impl gui::VariableColors for Stats {
	fn get(s: &str) -> Option<Color> {
		match s {
			"heart" => Some(HEART_COLOR),
			"soul" => Some(SOUL_COLOR),
			"power" => Some(POWER_COLOR),
			"defense" => Some(DEFENSE_COLOR),
			"magic" => Some(MAGIC_COLOR),
			"resistance" => Some(RESISTANCE_COLOR),
			_ => None,
		}
	}
}
