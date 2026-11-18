use std::sync::Arc;

use futures::FutureExt as _;
use language::{LanguageLoader, LanguageRegistry, LoadedLanguage};

mod rust;

/// Registers the grammar and language server the agent's code tools read.
pub fn init(languages: &Arc<LanguageRegistry>) {
    // One grammar, because `grammars`'s `load-grammars` feature would carry every
    // grammar into a binary that opens none of them.
    languages.register_native_grammars([("rust", tree_sitter_rust::LANGUAGE)]);

    let config = grammars::load_config_for_feature("rust", true);
    languages.register_lsp_adapter(config.name.clone(), Arc::new(rust::RustLspAdapter));

    let load: LanguageLoader = {
        let config = config.clone();
        Arc::new(move || {
            let config = config.clone();
            async move {
                Ok(LoadedLanguage {
                    config,
                    queries: grammars::load_queries("rust"),
                    context_provider: None,
                    toolchain_provider: None,
                    manifest_name: None,
                })
            }
            .boxed()
        })
    };

    languages.register_language(
        config.name.clone(),
        config.grammar.clone(),
        config.matcher.clone(),
        config.hidden,
        None,
        load,
    );
}
