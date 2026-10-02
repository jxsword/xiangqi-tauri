//! game-store: 持久化（多槽位存档、自动保存、FEN 导入导出）

pub fn placeholder() -> &'static str {
    "game-store skeleton"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ok() {
        assert!(placeholder().contains("game-store"));
    }
}
