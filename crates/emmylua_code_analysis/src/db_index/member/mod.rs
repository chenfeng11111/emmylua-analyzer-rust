mod lua_member;
mod lua_member_feature;
mod lua_member_item;
mod lua_member_owner;
mod lua_owner_members;

use hashbrown::{HashMap, HashSet};

use super::traits::LuaIndex;
use crate::{FileId, db_index::member::lua_owner_members::LuaOwnerMembers, DbIndex, LuaTypeDeclId, LuaType, InferGuard};
pub use lua_member::{LuaMember, LuaMemberId, LuaMemberKey};
pub use lua_member_feature::LuaMemberFeature;
pub use lua_member_item::LuaMemberIndexItem;
pub use lua_member_owner::LuaMemberOwner;

#[derive(Debug)]
pub struct LuaMemberIndex {
    members: HashMap<LuaMemberId, LuaMember>,
    in_filed: HashMap<FileId, HashSet<MemberOrOwner>>,
    owner_members: HashMap<LuaMemberOwner, LuaOwnerMembers>,
    member_current_owner: HashMap<LuaMemberId, LuaMemberOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MemberOrOwner {
    Member(LuaMemberId),
    Owner(LuaMemberOwner),
}

impl Default for LuaMemberIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaMemberIndex {
    pub fn new() -> Self {
        Self {
            members: HashMap::new(),
            in_filed: HashMap::new(),
            owner_members: HashMap::new(),
            member_current_owner: HashMap::new(),
        }
    }

    pub fn add_member(&mut self, owner: LuaMemberOwner, member: LuaMember) -> LuaMemberId {
        let id = member.get_id();
        let file_id = member.get_file_id();
        self.members.insert(id, member);
        self.add_in_file_object(file_id, MemberOrOwner::Member(id));
        if !owner.is_unknown() {
            self.member_current_owner.insert(id, owner.clone());
            self.add_in_file_object(file_id, MemberOrOwner::Owner(owner.clone()));
            self.add_member_to_owner(owner.clone(), id);
        }
        id
    }

    fn add_in_file_object(&mut self, file_id: FileId, member_or_owner: MemberOrOwner) {
        self.in_filed
            .entry(file_id)
            .or_default()
            .insert(member_or_owner);
    }

    pub fn add_member_to_owner(&mut self, owner: LuaMemberOwner, id: LuaMemberId) -> Option<()> {
        let member = self.get_member(&id)?;
        let key = member.get_key().clone();
        let feature = member.get_feature();
        let member_map = self
            .owner_members
            .entry(owner.clone())
            .or_insert_with(LuaOwnerMembers::new);
        if feature.is_decl() {
            if let Some(item) = member_map.get_member_mut(&key) {
                match item {
                    LuaMemberIndexItem::One(old_id) => {
                        if old_id != &id {
                            let ids = vec![*old_id, id];
                            *item = LuaMemberIndexItem::Many(ids);
                        }
                    }
                    LuaMemberIndexItem::Many(ids) => {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            } else {
                member_map.add_member(key.clone(), LuaMemberIndexItem::One(id));
            }
        } else {
            if !member_map.contains_member(&key) {
                member_map.add_member(key, LuaMemberIndexItem::One(id));
                return Some(());
            }

            let item = member_map.get_member(&key)?.clone();
            let new_items = if self.is_item_only_meta(&item) {
                match item {
                    LuaMemberIndexItem::One(old_id) => {
                        if old_id == id {
                            return Some(());
                        }
                        LuaMemberIndexItem::Many(vec![id, old_id])
                    }
                    LuaMemberIndexItem::Many(mut ids) => {
                        if ids.contains(&id) {
                            return Some(());
                        }

                        ids.push(id);
                        LuaMemberIndexItem::Many(ids)
                    }
                }
            } else {
                return Some(());
            };

            self.owner_members
                .entry(owner.clone())
                .or_insert_with(LuaOwnerMembers::new)
                .add_member(key.clone(), new_items);
        }

        Some(())
    }

    fn is_item_only_meta(&self, item: &LuaMemberIndexItem) -> bool {
        match item {
            LuaMemberIndexItem::One(id) => {
                if let Some(member) = self.get_member(id) {
                    return member.get_feature().is_meta_decl();
                }
            }
            LuaMemberIndexItem::Many(ids) => {
                for id in ids {
                    if let Some(member) = self.get_member(id)
                        && !member.get_feature().is_meta_decl()
                    {
                        return false;
                    }
                }
                return true;
            }
        }

        false
    }

    pub fn set_member_owner(
        &mut self,
        owner: LuaMemberOwner,
        file_id: FileId,
        id: LuaMemberId,
    ) -> Option<()> {
        self.member_current_owner.insert(id, owner.clone());
        self.add_in_file_object(file_id, MemberOrOwner::Owner(owner));

        Some(())
    }

    pub fn get_member(&self, id: &LuaMemberId) -> Option<&LuaMember> {
        self.members.get(id)
    }

    pub fn get_member_mut(&mut self, id: &LuaMemberId) -> Option<&mut LuaMember> {
        self.members.get_mut(id)
    }

    pub fn get_members(&self, owner: &LuaMemberOwner) -> Option<Vec<&LuaMember>> {
        let member_items = self.owner_members.get(owner)?;
        let mut members = Vec::new();
        for item in member_items.get_member_items() {
            match item {
                LuaMemberIndexItem::One(id) => {
                    if let Some(member) = self.get_member(id) {
                        members.push(member);
                    }
                }
                LuaMemberIndexItem::Many(ids) => {
                    for id in ids {
                        if let Some(member) = self.get_member(id) {
                            members.push(member);
                        }
                    }
                }
            }
        }

        Some(members)
    }

    #[allow(unused)]
    pub fn get_member_item_by_member_id(
        &self,
        member_id: LuaMemberId,
    ) -> Option<&LuaMemberIndexItem> {
        let owner = self.member_current_owner.get(&member_id)?;
        let member_key = self.members.get(&member_id)?.get_key();
        let member_items = self.owner_members.get(owner)?;
        let item = member_items.get_member(member_key)?;
        Some(item)
    }

    pub fn get_sorted_members(&self, owner: &LuaMemberOwner) -> Option<Vec<&LuaMember>> {
        let mut members = self.get_members(owner)?;
        members.sort_by_key(|member| member.get_sort_key());
        Some(members)
    }

    pub fn get_member_item(
        &self,
        owner: &LuaMemberOwner,
        key: &LuaMemberKey,
    ) -> Option<&LuaMemberIndexItem> {
        self.owner_members
            .get(owner)
            .and_then(|map| map.get_member(key))
    }

    pub fn get_member_len(&self, owner: &LuaMemberOwner) -> usize {
        self.owner_members
            .get(owner)
            .map_or(0, |map| map.get_member_len())
    }

    pub fn get_current_owner(&self, id: &LuaMemberId) -> Option<&LuaMemberOwner> {
        self.member_current_owner.get(id)
    }

    // 查询ctor函数成员
    pub fn get_constructor_call_func(
        &self,
        db: &DbIndex,
        prefix_type_id: &LuaTypeDeclId,
    ) -> Option<&LuaMemberIndexItem> {
        // 准备搜索"ctor"成员
        let ctor_key = LuaMemberKey::Name(SmolStr::new("ctor"));
        let current_owner = LuaMemberOwner::Type(prefix_type_id.clone());
        let type_index = db.get_type_index();

        // 使用广度优先搜索遍历类继承层次
        let mut visited = HashSet::new();
        let mut owners_to_check = vec![current_owner];

        while let Some(owner) = owners_to_check.pop() {
            if !visited.insert(owner.clone()) {
                continue;
            }

            // 检查当前类是否有ctor成员
            if let Some(member_item) = self.get_member_item(&owner, &ctor_key) {
                // 检查成员是否是函数类型
                if let Ok(typ) = member_item.resolve_type(db) {
                    match typ {
                        LuaType::Signature(_) | LuaType::DocFunction(_) => {
                            return Some(member_item);
                        },
                        _ => {}
                    }
                }
            }

            // 将父类添加到待检查列表
            if let LuaMemberOwner::Type(type_id) = &owner {
                if let Some(super_types) = type_index.get_super_types(type_id) {
                    for super_type in super_types {
                        if let LuaType::Ref(super_id) | LuaType::Def(super_id) = super_type {
                            owners_to_check.push(LuaMemberOwner::Type(super_id.clone()));
                        }
                    }
                }
            }
        }

        None
    }

    // LuaBehavior 将args中的值映射到自身
    pub fn get_member_lua_behavior(
        &self,
        db: &DbIndex,
        prefix_type_id: &LuaTypeDeclId,
        member_key: &LuaMemberKey,
    ) -> Option<&LuaMemberIndexItem> {
        // 只处理以下划线开头的成员名
        let name_str = member_key.get_name()?;
        if !name_str.starts_with('_') {
            return None;
        }

        // 检查类型是否有 LuaBehavior 特性
        let type_index = db.get_type_index();
        let is_lua_behavior = type_index
            .get_type_decl(prefix_type_id)
            .map_or(false, |decl| decl.is_lua_behavior(db, &InferGuard::new()));

        if !is_lua_behavior {
            return None;
        }

        // 处理args字段
        let owner = LuaMemberOwner::Type(prefix_type_id.clone());
        let args_member = self.get_member_item(&owner, &LuaMemberKey::Name(SmolStr::new("args")))?;

        // 解析 args 类型并查找对应的字段
        if let Ok(LuaType::TableConst(table_id)) = args_member.resolve_type(db) {
            let field_name = name_str.strip_prefix('_')?;
            let table_owner = LuaMemberOwner::Element(table_id);
            self.get_member_item(&table_owner, &LuaMemberKey::Name(SmolStr::new(field_name)))
        } else {
            None
        }
    }
}

impl LuaIndex for LuaMemberIndex {
    fn remove(&mut self, file_id: FileId) {
        if let Some(member_ids) = self.in_filed.remove(&file_id) {
            let mut owners = HashSet::new();
            for member_id_or_owner in member_ids {
                match member_id_or_owner {
                    MemberOrOwner::Member(member_id) => {
                        self.members.remove(&member_id);
                        self.member_current_owner.remove(&member_id);
                    }
                    MemberOrOwner::Owner(owner) => {
                        owners.insert(owner);
                    }
                }
            }

            let mut need_removed_owner = Vec::new();
            for owner in owners {
                if let Some(member_items) = self.owner_members.get_mut(&owner) {
                    let mut need_removed_key = Vec::new();
                    for (key, item) in member_items.iter_mut() {
                        match item {
                            LuaMemberIndexItem::One(id) => {
                                if id.file_id == file_id {
                                    need_removed_key.push(key.clone());
                                }
                            }
                            LuaMemberIndexItem::Many(ids) => {
                                ids.retain(|id| id.file_id != file_id);
                                if ids.is_empty() {
                                    need_removed_key.push(key.clone());
                                }
                            }
                        }
                    }

                    for key in need_removed_key {
                        member_items.remove_member(&key);
                    }

                    if member_items.is_empty() {
                        need_removed_owner.push(owner);
                    }
                }
            }

            for owner in need_removed_owner {
                self.owner_members.remove(&owner);
            }
        }
    }

    fn clear(&mut self) {
        self.members.clear();
        self.in_filed.clear();
        self.owner_members.clear();
    }
}
