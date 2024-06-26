use crate::prelude::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Energy {
	/// Positive energy, like heat.
	Positive,
	/// Negative energy, like cold.
	Negative,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Harmony {
	/// Spells with unconventional, unpredictable effects.
	Chaos,
	/// Simple spells with predictable effects.
	Order,
}

/// A character's magical skills.
///
/// Only skill from each axis may be chosen, and the minor skill is optional.
#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
// This gives the Skillset a cool toml representation.
#[serde(untagged)]
pub enum Skillset {
	EnergyMajor {
		major: Energy,
		minor: Option<Harmony>,
	},
	HarmonyMajor {
		major: Harmony,
		minor: Option<Energy>,
	},
}

#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Affinity {
	/// No skillset matches; the spell is not castable.
	Uncastable,
	/// Only a minor skill matches; spell is difficult to cast.
	Weak,
	/// Only a major skill matches; spell is slightly more difficult to cast.
	Average,
	/// Both skills match; spell is easy to cast.
	Strong,
}

impl Affinity {
	pub fn magnitude(self, magnitude: u32) -> u32 {
		match self {
			Affinity::Uncastable => 0,
			Affinity::Weak => magnitude / 2,
			Affinity::Average => magnitude * 3 / 4,
			Affinity::Strong => magnitude,
		}
	}
}

impl mlua::UserData for Affinity {
	fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_method("weak", |_, this, ()| Ok(matches!(this, Affinity::Weak)));
		methods.add_method("average", |_, this, ()| {
			Ok(matches!(this, Affinity::Average))
		});
		methods.add_method("strong", |_, this, ()| Ok(matches!(this, Affinity::Strong)));
	}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Spell {
	pub name: String,
	pub icon: String,

	/// Whether the spell concentrates or disperses energy.
	pub energy: Energy,
	/// Whether the spell is ordered or chaotic.
	pub harmony: Harmony,

	/// This is also the cost of the spell.
	pub level: u8,
	/// Parameters to the spell script.
	pub parameters: Parameters,
	/// Script to execute upon casting the spell.
	pub on_cast: script::MaybeInline,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Parameters {
	Target {
		/// Optional field for magnitude calculation.
		/// This could easily be part of a script,
		/// but expressions allow the magnitude formula to be displayed.
		magnitude: Option<Expression>,
		/// Amount by which defense must be beaten for damage to be dealt.
		/// Positive values filter out small spell magnitudes,
		/// wheras negative values counteract the target's resistance.
		///
		/// For example, a pierce threshold of 4 means that at least 4 damage
		/// must be dealt (after resistance) for an attack to land.
		/// A pierce threshold of -2 reduces the enemy's resistance by 2.
		#[serde(default)]
		pierce_threshold: i32,
	},
}

impl Spell {
	pub fn castable_by(&self, character: &character::Piece) -> bool {
		// if this ever changes, a result should be returned instead to print more detailed messages.
		character.sp >= self.level as i32
	}

	pub fn affinity(&self, character: &character::Piece) -> Affinity {
		match character.sheet.skillset {
			Skillset::EnergyMajor { major, minor } => {
				let minor_affinity = minor.is_some_and(|x| x == self.harmony);
				if major == self.energy {
					if minor_affinity {
						Affinity::Strong
					} else {
						Affinity::Average
					}
				} else if minor_affinity {
					Affinity::Weak
				} else {
					Affinity::Uncastable
				}
			}
			Skillset::HarmonyMajor { major, minor } => {
				let minor_affinity = minor.is_some_and(|x| x == self.energy);
				if major == self.harmony {
					if minor_affinity {
						Affinity::Strong
					} else {
						Affinity::Average
					}
				} else if minor_affinity {
					Affinity::Weak
				} else {
					Affinity::Uncastable
				}
			}
		}
	}
}
