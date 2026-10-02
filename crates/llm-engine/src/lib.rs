//! llm-engine: 大模型引擎（OpenAI 兼容 Chat Completions 客户端）
//! 大模型提议着法 -> 合法性校验 -> 失败/超时降级

pub fn placeholder() -> &'static str {
    "llm-engine skeleton"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ok() {
        assert!(placeholder().contains("llm-engine"));
    }
}
