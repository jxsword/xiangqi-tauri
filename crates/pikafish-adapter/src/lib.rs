//! pikafish-adapter: 皮卡鱼 UCI 子进程适配（桌面端增强引擎，GPLv3）

pub fn placeholder() -> &'static str {
    "pikafish-adapter skeleton"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ok() {
        assert!(placeholder().contains("pikafish-adapter"));
    }
}
