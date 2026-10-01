//! 引擎基线用例桥接测试（nautilus-baseline-tests）。
//!
//! 规格：logos/resources/test/unit/unit-test-cases.md、scenario-test-cases.md。
//! 每条用例一个集成测试，用 nautilus-research-testkit 的 `case()` 包装，
//! 结果写入 logos/resources/verify/test-results.jsonl（OpenLogos reporter）。
//! 断言直接调用引擎 crate 公开 API 复现验收点，不引用既有测试内部。
