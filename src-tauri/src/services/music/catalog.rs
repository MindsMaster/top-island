use island_core::LyricsData;

use super::lyrics as api;
use super::provider::TrackMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Catalog {
    Netease,
}

impl Catalog {
    fn prefix(self) -> &'static str {
        match self {
            Catalog::Netease => "ncm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongRef {
    pub catalog: Catalog,
    pub id: String,
}

impl SongRef {
    pub fn new(catalog: Catalog, id: impl Into<String>) -> Self {
        Self {
            catalog,
            id: id.into(),
        }
    }

    /// 前端 songId 与 lyricsId 同用此值
    pub fn key(&self) -> String {
        format!("{}:{}", self.catalog.prefix(), self.id)
    }
}

pub fn detail(song: &SongRef) -> Option<TrackMeta> {
    match song.catalog {
        Catalog::Netease => api::fetch_163_detail(&song.id),
    }
}

pub fn lyrics(song: &SongRef) -> Option<LyricsData> {
    match song.catalog {
        Catalog::Netease => api::fetch_163_by_id(&song.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_carries_catalog_prefix() {
        assert_eq!(SongRef::new(Catalog::Netease, "42").key(), "ncm:42");
    }
}
