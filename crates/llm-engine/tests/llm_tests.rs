//! LLM 引擎集成测试：用 wiremock 模拟 OpenAI 兼容端点
//! 覆盖：正常/含解释/非法着重试成功/两次非法/超时/500/连接拒绝

use llm_engine::{LlmClient, LlmConfig, LlmError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use xiangqi_core::board::Board;

fn client(server: &MockServer) -> LlmClient {
    LlmClient::new_no_proxy(LlmConfig::new(server.uri(), "test-key", "test-model"))
}

fn board() -> Board {
    Board::start_position()
}

fn ok_body(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "choices": [{ "message": { "content": content } }]
    }))
}

#[tokio::test]
async fn valid_move_ok() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_body("h2e2"))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s);
    let mv = c.propose_move(&board()).await.unwrap();
    assert_eq!(mv.to_ucci(), "h2e2");
}

#[tokio::test]
async fn explanation_tolerated() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_body("我建议 h2e2，这是最佳开局"))
        .mount(&s)
        .await;
    let c = client(&s);
    let mv = c.propose_move(&board()).await.unwrap();
    assert_eq!(mv.to_ucci(), "h2e2");
}

#[tokio::test]
async fn illegal_then_retry_success() {
    // 第一轮（无"无效"提示）非法 h8e8；重试轮（body 含"无效"）合法 h2e2
    use wiremock::{Match, Request};
    struct Hint(bool);
    impl Match for Hint {
        fn matches(&self, request: &Request) -> bool {
            let body = String::from_utf8_lossy(&request.body).to_string();
            body.contains("无效") == self.0
        }
    }
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(Hint(false))
        .respond_with(ok_body("h8e8"))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(Hint(true))
        .respond_with(ok_body("h2e2"))
        .mount(&s)
        .await;
    let c = client(&s);
    let mv = c.propose_move(&board()).await.unwrap();
    assert_eq!(mv.to_ucci(), "h2e2", "重试后应返回合法着");
}

#[tokio::test]
async fn illegal_twice_returns_error() {
    // v2：默认 3 次尝试全部非法 → IllegalMove
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_body("h8e8"))
        .expect(3)
        .mount(&s)
        .await;
    let c = client(&s);
    let err = c.propose_move(&board()).await.unwrap_err();
    assert!(
        matches!(err, LlmError::IllegalMove(_)),
        "三次非法应报非法着：{err}"
    );
}

#[tokio::test]
async fn no_move_in_reply() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_body("我不知道"))
        .mount(&s)
        .await;
    let c = client(&s);
    let err = c.propose_move(&board()).await.unwrap_err();
    assert!(matches!(err, LlmError::NoMoveInReply(_)), "{err}");
}

#[tokio::test]
async fn server_500_falls_to_status_error() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&s)
        .await;
    let c = client(&s);
    let err = c.propose_move(&board()).await.unwrap_err();
    assert!(matches!(err, LlmError::Status(500, _)), "{err}");
}

#[tokio::test]
async fn timeout_returns_timeout_error() {
    // 服务端延迟 > 超时（超时设为 1s，延迟 3s）
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"choices":[{"message":{"content":"h2e2"}}]}))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .mount(&s)
        .await;
    let mut cfg = LlmConfig::new(s.uri(), "k", "m");
    cfg.timeout_secs = 1;
    let c = LlmClient::new(cfg);
    let err = c.propose_move(&board()).await.unwrap_err();
    assert!(matches!(err, LlmError::Timeout(1)), "应报超时：{err}");
}

#[tokio::test]
async fn connection_refused_returns_http_error() {
    // 监听一个端口后关闭 → 连接拒绝
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let c = LlmClient::new_no_proxy(LlmConfig::new(format!("http://{addr}"), "k", "m"));
    let err = c.propose_move(&board()).await.unwrap_err();
    assert!(matches!(err, LlmError::Http(_)), "{err}");
}

#[tokio::test]
async fn advised_candidate_prompt_shape() {
    // candidate 模式：请求体必须包含候选清单标题、分档与两段式输出要求
    use wiremock::{Match, Request};
    struct BodyContains(&'static str);
    impl Match for BodyContains {
        fn matches(&self, request: &Request) -> bool {
            String::from_utf8_lossy(&request.body).contains(self.0)
        }
    }
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(BodyContains("候选着法清单"))
        .and(BodyContains("最佳/均势"))
        .and(BodyContains("着法: 起点-终点"))
        .and(BodyContains("【棋盘图】"))
        .respond_with(ok_body("着法: h2-e2"))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s);
    let ctx = llm_engine::LlmRequestContext {
        ascii_board: None,
        history_ucci: vec!["h2e2".into(), "h9g7".into()],
        candidate_moves: vec![("h2e2".into(), 12), ("h0g2".into(), -40)],
        full_legal: vec!["h2e2".into(), "h0g2".into()],
        veto_reason: None,
        repetition_warning: None,
    };
    let mv = c.propose_move_advised(&board(), &ctx).await.unwrap();
    assert_eq!(mv.to_ucci(), "h2e2");
}

#[tokio::test]
async fn sync_wrapper_works() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_body("h2e2"))
        .mount(&s)
        .await;
    let c = client(&s);
    let mv = c.propose_move_sync(&board()).unwrap();
    assert_eq!(mv.to_ucci(), "h2e2");
}

#[tokio::test]
async fn no_legal_moves_reports_bad_format() {
    // 轮黑已被将死的合法杀形局面：无合法着 → 不发请求直接报错
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&s)
        .await;
    let c = client(&s);
    let b = Board::parse_fen("3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 b - - 0 1").unwrap();
    assert!(b.legal_moves().is_empty());
    let err = c.propose_move(&b).await.unwrap_err();
    assert!(matches!(err, LlmError::BadFormat(_)), "{err}");
}
