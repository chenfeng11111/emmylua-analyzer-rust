use emmylua_code_analysis::{InferGuard, LuaDeclId, LuaMemberId, LuaMemberKey, LuaSemanticDeclId, LuaType, SemanticModel};
use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaFuncStat, LuaLocalFuncStat, LuaVarExpr};
use lsp_types::CodeLens;
use crate::context::ClientId;
use super::CodeLensData;

pub fn build_code_lens(
    semantic_model: &SemanticModel,
    client_id: ClientId,
) -> Option<Vec<CodeLens>> {
    let mut result = Vec::new();
    let root = semantic_model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaFuncStat(func_stat) => {
                let clone = func_stat.clone();
                add_func_stat_code_lens(semantic_model, &mut result, func_stat)?;
                match client_id {
                    ClientId::VSCode => {}
                    _ => {
                        add_func_stat_override_code_lens(semantic_model, &mut result, clone)?;
                    }
                }
            }
            LuaAst::LuaLocalFuncStat(local_func_stat) => {
                add_local_func_stat_code_lens(semantic_model, &mut result, local_func_stat)?;
            }
            _ => {}
        }
    }

    Some(result)
}

fn add_func_stat_code_lens(
    semantic_model: &SemanticModel,
    result: &mut Vec<CodeLens>,
    func_stat: LuaFuncStat,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();
    let func_name = func_stat.get_func_name()?;
    let document = semantic_model.get_document();
    match func_name {
        LuaVarExpr::IndexExpr(index_expr) => {
            let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
            let data = CodeLensData::Member(member_id);
            let index_name_token = index_expr.get_index_name_token()?;
            let range = document.to_lsp_range(index_name_token.text_range())?;
            result.push(CodeLens {
                range,
                command: None,
                data: Some(serde_json::to_value(data).unwrap()),
            });
        }
        LuaVarExpr::NameExpr(name_expr) => {
            let name_token = name_expr.get_name_token()?;
            let decl_id = LuaDeclId::new(file_id, name_token.get_position());
            let data = CodeLensData::DeclId(decl_id);
            let range = document.to_lsp_range(name_token.get_range())?;
            result.push(CodeLens {
                range,
                command: None,
                data: Some(serde_json::to_value(data).unwrap()),
            });
        }
    }

    Some(())
}

fn add_local_func_stat_code_lens(
    semantic_model: &SemanticModel,
    result: &mut Vec<CodeLens>,
    local_func_stat: LuaLocalFuncStat,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();
    let func_name = local_func_stat.get_local_name()?;
    let document = semantic_model.get_document();
    let range = document.to_lsp_range(func_name.get_range())?;
    let name_token = func_name.get_name_token()?;
    let decl_id = LuaDeclId::new(file_id, name_token.get_position());
    let data = CodeLensData::DeclId(decl_id);
    result.push(CodeLens {
        range,
        command: None,
        data: Some(serde_json::to_value(data).unwrap()),
    });
    Some(())
}

fn add_func_stat_override_code_lens(
    semantic_model: &SemanticModel,
    result: &mut Vec<CodeLens>,
    func_stat: LuaFuncStat,
) -> Option<()> {
    let func_name = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let prefix_expr = index_expr.get_prefix_expr()?;
        let prefix_type = semantic_model.infer_expr(prefix_expr.into()).ok()?;
        if let LuaType::Def(id) = prefix_type {
            let supers = semantic_model
                .get_db()
                .get_type_index()
                .get_super_types(&id)?;

            let index_key = index_expr.get_index_key()?;
            let member_key: LuaMemberKey = semantic_model.get_member_key(&index_key)?;
            let infer_guard = &mut InferGuard::new();
            for super_type in supers {
                if let Some(member_id) =
                    get_super_member_id(semantic_model, super_type, &member_key, infer_guard)
                {
                    let document = semantic_model.get_document();
                    let index_name_token = index_expr.get_index_name_token()?;
                    let range = document.to_lsp_range(index_name_token.text_range())?;

                    // 使用新的 Override 变体
                    let data = CodeLensData::Override(member_id);
                    result.push(CodeLens {
                        range,
                        command: None,
                        data: Some(serde_json::to_value(data).unwrap()),
                    });
                    break;
                }
            }
        }
    }

    Some(())
}

fn get_super_member_id(
    semantic_model: &SemanticModel,
    super_type: LuaType,
    member_key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> Option<LuaMemberId> {
    if let LuaType::Ref(super_type_id) = &super_type {
        infer_guard.check(super_type_id).ok()?;
        let member_map = semantic_model.get_member_info_map(&super_type)?;

        if let Some(member_infos) = member_map.get(&member_key) {
            let first_property = member_infos.first()?.property_owner_id.clone()?;
            if let LuaSemanticDeclId::Member(member_id) = first_property {
                return Some(member_id);
            }
        }
    }

    None
}

