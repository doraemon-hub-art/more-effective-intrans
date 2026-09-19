/*
 * @file translator.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Translator trait: the contract every translation provider implements
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::future::Future;

use crate::error::Result;

/// What every translation provider offers: Chinese text in, English text out.
///
/// The returned future is spelled out instead of using `async fn` in the trait, because the
/// implicit future of `async fn` carries no `Send` bound — and this one gets spawned onto a
/// background runtime from the UI thread, so it has to be `Send`. The spelling also keeps
/// the trait out of the dyn-compatibility question for now; when a registry has to hold
/// providers of different types behind one pointer, the return type becomes
/// `Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>`.
pub trait Translator: Send + Sync {
    /// Translates one piece of text. Provider-specific failures are mapped onto the shared
    /// error type, so callers treat every provider the same way.
    fn translate(&self, text: &str) -> impl Future<Output = Result<String>> + Send;
}
