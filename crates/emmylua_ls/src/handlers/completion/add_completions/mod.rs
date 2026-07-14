mod add_decl_completion;
mod add_member_completion;
mod check_match_word;

use std::ops::Add;
pub use add_decl_completion::add_decl_completion;
pub use add_member_completion::get_index_alias_name;
pub use add_member_completion::{CompletionTriggerStatus, add_member_completion};
pub use check_match_word::check_match_word;
use emmylua_code_analysis::{LuaSemanticDeclId, LuaType, RenderLevel};
use lsp_types::CompletionItemKind;

use super::completion_builder::CompletionBuilder;
use emmylua_code_analysis::LuaCommonProperty;
use emmylua_code_analysis::humanize_type;

pub fn check_visibility(builder: &mut CompletionBuilder, id: LuaSemanticDeclId) -> Option<()> {
    match id {
        LuaSemanticDeclId::Member(_) => {}
        LuaSemanticDeclId::LuaDecl(_) => {}
        _ => return Some(()),
    }

    if !builder
        .semantic_model
        .is_semantic_visible(builder.trigger_token.clone(), id)
    {
        return None;
    }

    Some(())
}

pub fn get_completion_kind(typ: &LuaType) -> CompletionItemKind {
    if typ.is_function() {
        return CompletionItemKind::FUNCTION;
    } else if typ.is_const() {
        return CompletionItemKind::CONSTANT;
    } else if typ.is_def() {
        return CompletionItemKind::CLASS;
    } else if typ.is_namespace() {
        return CompletionItemKind::MODULE;
    }

    CompletionItemKind::VARIABLE
}

pub fn is_deprecated(builder: &CompletionBuilder, id: LuaSemanticDeclId) -> bool {
    let property = builder
        .semantic_model
        .get_db()
        .get_property_index()
        .get_property(&id);

    property.is_some_and(property_is_deprecated)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CallDisplay {
    None,
    AddSelf,
    RemoveFirst,
}

pub fn get_detail(
    builder: &CompletionBuilder,
    typ: &LuaType,
    display: CallDisplay,
    show_literal_params: bool,
) -> Option<String> {
    let db = builder.semantic_model.get_db();
    let param_text = |param: &(String, Option<LuaType>)| {
        if show_literal_params
            && let Some(typ) = &param.1
            && matches!(
                typ,
                LuaType::Nil
                    | LuaType::BooleanConst(_)
                    | LuaType::StringConst(_)
                    | LuaType::IntegerConst(_)
                    | LuaType::FloatConst(_)
                    | LuaType::DocStringConst(_)
                    | LuaType::DocIntegerConst(_)
                    | LuaType::DocBooleanConst(_)
            )
        {
            return humanize_type(db, typ, RenderLevel::Minimal);
        }

        param.0.clone()
    };

    match typ {
        LuaType::Signature(signature_id) => {
            let signature = builder
                .semantic_model
                .get_db()
                .get_signature_index()
                .get(signature_id)?;

            let mut params_str = signature
                .get_type_params()
                .iter()
                .map(param_text)
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }
            let rets = &signature.return_docs;
            let rets_detail = if rets.len() == 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0].type_ref,
                    RenderLevel::Minimal,
                );
                format!(" -> {}", detail)
            } else if rets.len() > 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0].type_ref,
                    RenderLevel::Minimal,
                );
                format!(" -> {} ...", detail)
            } else {
                "".to_string()
            };

            Some(format!("({}){}", params_str.join(", "), rets_detail))
        }
        LuaType::DocFunction(f) => {
            let mut params_str = f.get_params().iter().map(param_text).collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }
            let ret_type = f.get_ret();
            let rets_detail = match ret_type {
                LuaType::Nil => "".to_string(),
                _ => {
                    let type_detail = humanize_type(
                        builder.semantic_model.get_db(),
                        ret_type,
                        RenderLevel::Minimal,
                    );
                    format!("-> {}", type_detail)
                }
            };
            Some(format!("({}){}", params_str.join(", "), rets_detail))
        }
        _ => None,
    }
}

pub fn get_function_snippet(
    builder: &CompletionBuilder,
    label: &str,
    typ: &LuaType,
    display: CallDisplay,
) -> Option<String> {
    match typ {
        LuaType::Signature(signature_id) => {
            let signature = builder
                .semantic_model
                .get_db()
                .get_signature_index()
                .get(signature_id)?;

            let mut params_str = signature
                .get_type_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }

            Some(format!(
                "{}({})",
                label,
                params_str
                    .iter()
                    .enumerate()
                    .map(|(i, name)| format!("${{{}:{}}}", i + 1, name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
        LuaType::DocFunction(f) => {
            let mut params_str = f
                .get_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }

            Some(format!(
                "{}({})",
                label,
                params_str
                    .iter()
                    .enumerate()
                    .map(|(i, name)| format!("${{{}:{}}}", i + 1, name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
        _ => None,
    }
}

fn get_description(builder: &CompletionBuilder, typ: &LuaType) -> Option<String> {
    match typ {
        LuaType::Signature(_) => None,
        LuaType::DocFunction(_) => None,
        _ if typ.is_unknown() => None,
        _ => Some(humanize_type(
            builder.semantic_model.get_db(),
            typ,
            RenderLevel::Minimal,
        )),
    }
}

fn get_function_insert_text(
    builder: &CompletionBuilder,
    display: CallDisplay,
    label: &str,
    need_parentheses: bool,
    typ: &LuaType,
) -> String {
    let end = builder.trigger_token.text_range().end();

    let document = builder.semantic_model.get_document();
    let text_len = document.get_valid_range();

    let next_is_parenthesis = if end.add(TextSize::new(1)) > TextSize::new(text_len) {
        false
    } else {
        let range = TextRange::new(end, end.add(TextSize::new(1)));
        let char = document.get_text_slice(range);
        char == "("
    };

    if !next_is_parenthesis
        && need_parentheses
        && (matches!(typ, LuaType::DocFunction(_)) || matches!(typ, LuaType::Signature(_)))
    {
        let mut param_count = match typ {
            LuaType::DocFunction(func) => func.get_params().len(),
            LuaType::Signature(signature_id) => {
                let signature = builder
                    .semantic_model
                    .get_db()
                    .get_signature_index()
                    .get(&signature_id);
                if let Some(value) = signature {
                    value.get_type_params().len()
                } else {
                    0
                }
            }
            _ => 0,
        };

        match display {
            CallDisplay::AddSelf => {
                param_count += 1;
            }
            CallDisplay::RemoveFirst => {
                if param_count > 0 {
                    param_count -= 1;
                }
            }
            _ => {}
        }
        if param_count > 0 {
            format!("{}(${{1}})${{0}}", label)
        } else {
            format!("{}()", label)
        }
    } else {
        label.to_string()
    }
}

fn property_is_deprecated(property: &LuaCommonProperty) -> bool {
    property.deprecated().is_some()
        || property.attribute_uses().is_some_and(|attribute_uses| {
            attribute_uses
                .iter()
                .any(|attribute_use| attribute_use.as_deprecated().is_some())
        })
}
