mod check_reason;
mod infer_manager;
mod resolve;
mod resolve_closure;

use crate::{
    db_index::{DbIndex, LuaDeclId, LuaMemberId, LuaSignatureId},
    profile::Profile,
    FileId, InferFailReason, LuaMemberFeature, LuaSemanticDeclId,
};
use check_reason::resolve_all_reason;
use emmylua_parser::{
    LuaAssignStat, LuaCallExpr, LuaExpr, LuaFuncStat, LuaTableExpr, LuaTableField,
};
use infer_manager::InferCacheManager;
use resolve::{
    try_resolve_decl, try_resolve_iter_var, try_resolve_member, try_resolve_module,
    try_resolve_module_ref, try_resolve_return_point, try_resolve_table_field,
};
use resolve_closure::{
    try_resolve_closure_params, try_resolve_closure_parent_params, try_resolve_closure_return,
};

use super::{lua::LuaReturnPoint, AnalyzeContext};

pub fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
    let _p = Profile::cond_new("resolve analyze", context.tree_list.len() > 1);
    let mut unresolves = std::mem::take(&mut context.unresolves);
    let mut infer_manager = InferCacheManager::new();

    let mut loop_count = 0;
    while !unresolves.is_empty() {
        while try_resolve(db, &mut infer_manager, &mut unresolves) {
            unresolves.retain(|un_resolve| match un_resolve {
                UnResolve::None => false,
                _ => true,
            });
        }
        if loop_count == 0 {
            infer_manager.set_force();
        }

        resolve_all_reason(db, &mut infer_manager, &mut unresolves);

        if loop_count >= 10 {
            break;
        }
        loop_count += 1;
    }
}

fn try_resolve(
    db: &mut DbIndex,
    infer_manager: &mut InferCacheManager,
    unresolves: &mut Vec<UnResolve>,
) -> bool {
    let mut changed = false;
    for i in 0..unresolves.len() {
        let un_resolve = &mut unresolves[i];
        let file_id = un_resolve.get_file_id().unwrap_or(FileId { id: 0 });
        let config = infer_manager.get_infer_cache(file_id);
        let resolve = match un_resolve {
            UnResolve::Decl(un_resolve_decl) => {
                try_resolve_decl(db, config, un_resolve_decl).unwrap_or(false)
            }
            UnResolve::Member(ref mut un_resolve_member) => {
                try_resolve_member(db, config, un_resolve_member).unwrap_or(false)
            }
            UnResolve::Module(un_resolve_module) => {
                try_resolve_module(db, config, un_resolve_module).unwrap_or(false)
            }
            UnResolve::Return(un_resolve_return) => {
                try_resolve_return_point(db, config, un_resolve_return).unwrap_or(false)
            }
            UnResolve::ClosureParams(un_resolve_closure_params) => {
                try_resolve_closure_params(db, config, un_resolve_closure_params).unwrap_or(false)
            }
            UnResolve::ClosureReturn(un_resolve_closure_return) => {
                try_resolve_closure_return(db, config, un_resolve_closure_return).unwrap_or(false)
            }
            UnResolve::IterDecl(un_resolve_iter_var) => {
                try_resolve_iter_var(db, config, un_resolve_iter_var).unwrap_or(false)
            }
            UnResolve::ModuleRef(module_ref) => {
                try_resolve_module_ref(db, config, module_ref).unwrap_or(false)
            }
            UnResolve::ClosureParentParams(un_resolve_closure_params) => {
                try_resolve_closure_parent_params(db, config, un_resolve_closure_params)
                    .unwrap_or(false)
            }
            UnResolve::TableField(un_resolve_table_field) => {
                try_resolve_table_field(db, config, un_resolve_table_field).unwrap_or(false)
            }
            UnResolve::None => continue,
        };

        if resolve {
            changed = true;
            unresolves[i] = UnResolve::None;
        }
    }

    changed
}

#[derive(Debug)]
pub enum UnResolve {
    None,
    Decl(Box<UnResolveDecl>),
    IterDecl(Box<UnResolveIterVar>),
    Member(Box<UnResolveMember>),
    Module(Box<UnResolveModule>),
    Return(Box<UnResolveReturn>),
    ClosureParams(Box<UnResolveCallClosureParams>),
    ClosureReturn(Box<UnResolveClosureReturn>),
    ClosureParentParams(Box<UnResolveParentClosureParams>),
    ModuleRef(Box<UnResolveModuleRef>),
    TableField(Box<UnResolveTableField>),
}

#[allow(dead_code)]
impl UnResolve {
    pub fn is_none(&self) -> bool {
        matches!(self, UnResolve::None)
    }

    pub fn get_file_id(&self) -> Option<FileId> {
        match self {
            UnResolve::Decl(un_resolve_decl) => Some(un_resolve_decl.file_id),
            UnResolve::IterDecl(un_resolve_iter_var) => Some(un_resolve_iter_var.file_id),
            UnResolve::Member(un_resolve_member) => Some(un_resolve_member.file_id),
            UnResolve::Module(un_resolve_module) => Some(un_resolve_module.file_id),
            UnResolve::Return(un_resolve_return) => Some(un_resolve_return.file_id),
            UnResolve::ClosureParams(un_resolve_closure_params) => {
                Some(un_resolve_closure_params.file_id)
            }
            UnResolve::ClosureReturn(un_resolve_closure_return) => {
                Some(un_resolve_closure_return.file_id)
            }
            UnResolve::ClosureParentParams(un_resolve_closure_params) => {
                Some(un_resolve_closure_params.file_id)
            }
            UnResolve::TableField(un_resolve_table_field) => Some(un_resolve_table_field.file_id),
            UnResolve::ModuleRef(_) => None,
            UnResolve::None => None,
        }
    }
}

#[derive(Debug)]
pub struct UnResolveDecl {
    pub file_id: FileId,
    pub decl_id: LuaDeclId,
    pub expr: LuaExpr,
    pub ret_idx: usize,
    pub reason: InferFailReason,
}

impl From<UnResolveDecl> for UnResolve {
    fn from(un_resolve_decl: UnResolveDecl) -> Self {
        UnResolve::Decl(Box::new(un_resolve_decl))
    }
}

#[derive(Debug)]
pub struct UnResolveMember {
    pub file_id: FileId,
    pub member_id: LuaMemberId,
    pub expr: Option<LuaExpr>,
    pub prefix: Option<LuaExpr>,
    pub ret_idx: usize,
    pub reason: InferFailReason,
}

impl From<UnResolveMember> for UnResolve {
    fn from(un_resolve_member: UnResolveMember) -> Self {
        UnResolve::Member(Box::new(un_resolve_member))
    }
}

#[derive(Debug)]
pub struct UnResolveModule {
    pub file_id: FileId,
    pub expr: LuaExpr,
    pub reason: InferFailReason,
}

impl From<UnResolveModule> for UnResolve {
    fn from(un_resolve_module: UnResolveModule) -> Self {
        UnResolve::Module(Box::new(un_resolve_module))
    }
}

#[derive(Debug)]
pub struct UnResolveReturn {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub return_points: Vec<LuaReturnPoint>,
    pub reason: InferFailReason,
}

impl From<UnResolveReturn> for UnResolve {
    fn from(un_resolve_return: UnResolveReturn) -> Self {
        UnResolve::Return(Box::new(un_resolve_return))
    }
}

#[derive(Debug)]
pub struct UnResolveCallClosureParams {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub call_expr: LuaCallExpr,
    pub param_idx: usize,
    pub reason: InferFailReason,
}

impl From<UnResolveCallClosureParams> for UnResolve {
    fn from(un_resolve_closure_params: UnResolveCallClosureParams) -> Self {
        UnResolve::ClosureParams(Box::new(un_resolve_closure_params))
    }
}

#[derive(Debug)]
pub struct UnResolveIterVar {
    pub file_id: FileId,
    pub decl_id: LuaDeclId,
    pub iter_expr: LuaExpr,
    pub ret_idx: usize,
    pub reason: InferFailReason,
}

impl From<UnResolveIterVar> for UnResolve {
    fn from(un_resolve_iter_var: UnResolveIterVar) -> Self {
        UnResolve::IterDecl(Box::new(un_resolve_iter_var))
    }
}

#[derive(Debug)]
pub struct UnResolveClosureReturn {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub call_expr: LuaCallExpr,
    pub param_idx: usize,
    pub return_points: Vec<LuaReturnPoint>,
    pub reason: InferFailReason,
}

impl From<UnResolveClosureReturn> for UnResolve {
    fn from(un_resolve_closure_return: UnResolveClosureReturn) -> Self {
        UnResolve::ClosureReturn(Box::new(un_resolve_closure_return))
    }
}

#[derive(Debug)]
pub struct UnResolveModuleRef {
    pub owner_id: LuaSemanticDeclId,
    pub module_file_id: FileId,
    pub reason: InferFailReason,
}

impl From<UnResolveModuleRef> for UnResolve {
    fn from(un_resolve_module_ref: UnResolveModuleRef) -> Self {
        UnResolve::ModuleRef(Box::new(un_resolve_module_ref))
    }
}

#[derive(Debug)]
pub enum UnResolveParentAst {
    LuaFuncStat(LuaFuncStat),
    LuaTableField(LuaTableField),
    LuaAssignStat(LuaAssignStat),
}

#[derive(Debug)]
pub struct UnResolveParentClosureParams {
    pub file_id: FileId,
    pub signature_id: LuaSignatureId,
    pub parent_ast: UnResolveParentAst,
    pub reason: InferFailReason,
}

impl From<UnResolveParentClosureParams> for UnResolve {
    fn from(un_resolve_closure_params: UnResolveParentClosureParams) -> Self {
        UnResolve::ClosureParentParams(Box::new(un_resolve_closure_params))
    }
}

#[derive(Debug)]
pub struct UnResolveTableField {
    pub file_id: FileId,
    pub table_expr: LuaTableExpr,
    pub field: LuaTableField,
    pub decl_feature: LuaMemberFeature,
    pub reason: InferFailReason,
}

impl From<UnResolveTableField> for UnResolve {
    fn from(un_resolve_table_field: UnResolveTableField) -> Self {
        UnResolve::TableField(Box::new(un_resolve_table_field))
    }
}
