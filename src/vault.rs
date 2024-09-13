use crate::floor::Tile;
use crate::prelude::*;
use std::{collections::HashMap, fs, path::Path};

pub struct Set {
	pub vaults: Vec<String>,
	/// Nodes per floor
	pub density: u32,
	/// ratio of halls to vaults.
	pub hall_ratio: i32,
}

#[derive(Clone, Debug)]
pub struct Vault {
	pub tiles: Vec<Option<Tile>>,
	pub width: usize,

	pub characters: Vec<(i32, i32, resource::Id)>,
	pub edges: Vec<(i32, i32)>,
}

fn tile_floor() -> Tile {
	Tile::Floor
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SymbolMeaning {
	Tile(Tile),
	Character {
		sheet: resource::Id,
		#[serde(default = "tile_floor")]
		tile: Tile,
	},
	Edge(Tile),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
	symbols: HashMap<char, SymbolMeaning>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("vault is missing a layout section")]
	MissingLayout,
	#[error("unexpected symbol: {0}")]
	UnexpectedSymbol(char),
}

impl Vault {
	pub fn height(&self) -> usize {
		self.tiles.len() / self.width
	}

	/// # Errors
	///
	/// Returns an error if the file could not be opened or parsed.
	pub fn open(path: impl AsRef<Path>) -> Result<Self> {
		let mut width = 0;

		let vault_text = fs::read_to_string(path)?;

		let (metadata, layout) = vault_text
			.split_once("# Layout\n")
			.ok_or(Error::MissingLayout)?;

		let metadata: Metadata = toml::from_str(metadata)?;

		// Before we can do anything, we need to know how wide this vault is.
		for line in layout.lines() {
			width = width.max(line.len());
		}

		let mut tiles = Vec::new();
		let mut characters = Vec::new();
		let mut edges = Vec::new();

		for (y, line) in layout.lines().enumerate() {
			for (x, c) in line.chars().enumerate() {
				if let Some(action) = metadata.symbols.get(&c) {
					match action {
						SymbolMeaning::Tile(t) => tiles.push(Some(*t)),
						SymbolMeaning::Edge(t) => {
							tiles.push(Some(*t));
							edges.push((x as i32, y as i32));
						}
						SymbolMeaning::Character { sheet, tile } => {
							characters.push((x as i32, y as i32, sheet.clone()));
							tiles.push(Some(*tile));
						}
					}
				} else {
					tiles.push(match c {
						' ' => None,
						'.' => Some(Tile::Floor),
						'x' => Some(Tile::Wall),
						'>' => Some(Tile::Exit),
						_ => Err(Error::UnexpectedSymbol(c))?,
					});
				}
			}
			for _ in 0..(width - line.len()) {
				tiles.push(None);
			}
		}

		Ok(Self {
			tiles,
			width,

			edges,
			characters,
		})
	}
}
