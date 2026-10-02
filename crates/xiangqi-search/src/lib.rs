//! xiangqi-search: 内置搜索引擎（迭代加深 α-β + 置换表 + 局面评估）

pub fn placeholder() -> &'static str {
    "xiangqi-search skeleton"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ok() {
        assert!(placeholder().contains("xiangqi-search"));
    }
}
