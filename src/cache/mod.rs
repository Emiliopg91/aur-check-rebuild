use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

const CACHE_FILE: &str = "/tmp/aur-check-rebuild/cache";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheEntry {
    pub dependencies: Vec<String>,
    pub db: String,
    pub so_files: Vec<String>,
    pub so_dependencies: HashMap<String, Vec<String>>,
}

impl Default for CacheEntry {
    fn default() -> Self {
        Self {
            dependencies: Vec::new(),
            db: "unknown".to_string(),
            so_files: Vec::new(),
            so_dependencies: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cache {
    pub data: HashMap<String, CacheEntry>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl Cache {
    pub fn load() -> Result<Cache, std::io::Error> {
        let data = fs::read_to_string(CACHE_FILE)?;
        let pkg: HashMap<_, _> = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        return Ok(Cache { data: pkg });
    }

    pub fn store(&self) -> Result<(), std::io::Error> {
        let path = Path::new(CACHE_FILE);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let ordered: BTreeMap<_, _> = self.data.clone().into_iter().collect();

        let data = serde_json::to_string_pretty(&ordered)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        fs::write(path, data)?;
        Ok(())
    }

    pub fn get_aur_packages(&self) -> Vec<String> {
        self.data
            .par_iter()
            .filter(|(_, v)| v.db == "unknown")
            .map(|(k, _)| k.clone())
            .collect()
    }
}
