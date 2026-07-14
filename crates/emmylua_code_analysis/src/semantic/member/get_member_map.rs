use std::collections::HashMap;

use crate::{DbIndex, FileId, LuaMemberKey, LuaType};

use super::{
    LuaMemberInfo,
    find_members::{self},
};

pub fn get_member_map(
    db: &DbIndex,
    prefix_type: &LuaType,
) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
    let members = find_members::find_members(db, prefix_type)?;
    build_member_map(members)
}

pub fn get_member_map_in_scope(
    db: &DbIndex,
    file_id: FileId,
    prefix_type: &LuaType,
) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
    let members = find_members::find_members_in_scope(db, file_id, prefix_type)?;
    build_member_map(members)
}

fn build_member_map(
    members: Vec<LuaMemberInfo>,
) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
    let mut member_map = HashMap::new();
    for member in members {
        let key = member.key.clone();
        let typ = &member.typ;
        // 通常是泛型实例化推断结果
        if let LuaType::Union(u) = typ
            && u.into_vec().iter().all(|f| f.is_function())
        {
            for (index, f) in u.into_vec().iter().enumerate() {
                let new_member = LuaMemberInfo {
                    key: key.clone(),
                    typ: f.clone(),
                    property_owner_id: member.property_owner_id.clone(),
                    feature: member.feature,
                    overload_index: Some(index),
                };

                member_map
                    .entry(key.clone())
                    .or_insert_with(Vec::new)
                    .push(new_member);
            }
            continue;
        }
        member_map.entry(key).or_insert_with(Vec::new).push(member);
    }

    Some(member_map)
}

pub fn get_lua_behavior_args_map(
    db: &DbIndex,
    prefix_type: &LuaType,
) -> Option<HashMap<LuaMemberKey, Vec<LuaMemberInfo>>> {
    // 只处理 Def 或 Ref 类型
    let type_decl_id = match prefix_type {
        LuaType::Def(id) | LuaType::Ref(id) => id,
        _ => return None,
    };

    // 获取类型声明并检查是否为 lua_behavior
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(type_decl_id)?;
    if !type_decl.is_lua_behavior(db, &InferGuard::new()) {
        return None;
    }

    // 获取 args 成员
    let owner = LuaMemberOwner::Type(type_decl_id.clone());
    let args_key = LuaMemberKey::Name(SmolStr::new("args"));
    let args_member = db.get_member_index().get_member_item(&owner, &args_key)?;

    // 解析 args 类型
    let args_type = args_member.resolve_type(db).ok()?;

    // 查找所有成员并转换为需要的格式
    let members = find_members::find_members(db, &args_type)?;

    // 构建成员映射
    let member_map = members.into_iter().map(|member| {
        // 重命名 key
        let new_key = match &member.key {
            LuaMemberKey::Name(name) => LuaMemberKey::Name(SmolStr::new(format!("_{}", name))),
            _ => member.key.clone(),
        };

        // 创建新的成员信息
        LuaMemberInfo {
            key: new_key.clone(),
            typ: member.typ,
            property_owner_id: member.property_owner_id,
            feature: member.feature,
            overload_index: member.overload_index,
        }
    }).fold(HashMap::new(), |mut map, member| {
        map.entry(member.key.clone())
            .or_insert_with(Vec::new)
            .push(member);
        map
    });

    Some(member_map)
}
