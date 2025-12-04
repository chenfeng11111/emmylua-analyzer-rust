use crate::diagnostic::checker::{Checker, DiagnosticContext};
use crate::{DiagnosticCode, LuaSignatureId, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaClosureExpr};
use std::collections::HashMap;

pub struct CheckParamType;

impl Checker for CheckParamType {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UnknownFunctionParam];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();

        for closure in root.descendants::<LuaClosureExpr>() {
            check_param_type(context, semantic_model, &closure);
        }
    }
}

fn check_param_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();
    let signature_id = LuaSignatureId::from_closure(file_id, &closure);
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;

    let lua_params = closure.get_params_list()?;
    let signature_params = signature.get_type_params();
    let mut lua_params_map = HashMap::new();
    for param in lua_params.get_params() {
        if let Some(name_token) = param.get_name_token() {
            let name = name_token.get_name_text().to_string();
            lua_params_map.insert(name, param);
        } else if param.is_dots() {
            lua_params_map.insert("...".to_string(), param);
        }
    }

    for (signature_param_name, typ) in &signature_params {
        if typ.is_none() {
            let is_disable = semantic_model
                .get_emmyrc()
                .diagnostics
                .disable_check_param_type
                .iter()
                .find(|&name| name == signature_param_name);
            if is_disable.is_some() {
                continue;
            }
            let param = lua_params_map.get(signature_param_name).unwrap();
            context.add_diagnostic(
                DiagnosticCode::UnknownFunctionParam,
                param.get_range(),
                t!(
                    "unknown function param type %{name}",
                    name = signature_param_name
                )
                .to_string(),
                None,
            );
        }
    }
    Some(())
}
