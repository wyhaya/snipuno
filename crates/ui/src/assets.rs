use super::Icon;
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .binary_search_by(|icon| icon.path().cmp(path))
            .ok()
            .map(|index| Cow::Borrowed(Icon::ALL[index].bytes())))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_paths_are_unique_and_sorted_for_binary_search() {
        let paths: Vec<_> = Icon::ALL.iter().map(|icon| icon.path()).collect();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(paths, sorted);
    }

    #[test]
    fn lookup_returns_the_requested_asset_or_none() {
        for icon in Icon::ALL {
            let bytes = IconAssets.load(icon.path()).unwrap().unwrap();
            assert!(!bytes.is_empty(), "{}", icon.path());
            assert_eq!(bytes.as_ref(), icon.bytes(), "{}", icon.path());
        }
        for path in ["", "icons/missing.svg", "../Cargo.toml"] {
            assert!(IconAssets.load(path).unwrap().is_none(), "{path}");
        }
    }
}
