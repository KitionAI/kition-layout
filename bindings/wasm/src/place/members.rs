//! Writes that rest on the set of items carrying a property (C8-layout review 3): `xml.deleteTag`
//! untags every item tagged with its name when it runs, and `form.set` picks a default field name
//! no other form field has. Items another writer tagged, or gave a form field, after the
//! observation would be untagged or avoided without refusal: the set is a place told by `covers`
//! (its members now as `objects`, each held at the property), so an item that joined or left it
//! since `basis` refuses the write.
use super::*;

pub(super) enum Members {
    /// Items tagged with this XML tag.
    XmlTag(String),
    /// Items that are form fields.
    FormFields,
    /// The top-level items on this layer, which deleting it deletes (`~layerItems`). An item
    /// another agent on this server put on the layer names neither the layer nor the root in its
    /// change set, so the per-layer derived value is not re-read; the set as a place is.
    Layer(u64),
}

impl Members {
    /// The set `command` reads, if it reads one with these parameters.
    pub(super) fn of(doc: &LayoutDocument, command: &str, params: &Value) -> Result<Option<Self>, String> {
        match command {
            "xml.deleteTag" => Ok(params.get("name").and_then(Value::as_str).map(|name| Self::XmlTag(name.to_owned()))),
            "layer.delete" => Ok(params.get("id").and_then(Value::as_u64).map(Self::Layer)),
            "form.set" => {
                // Only a default name reads the other fields' names.
                if params.get("name").is_some_and(|name| !name.is_null()) {
                    return Ok(None);
                }
                let state = doc.active()?;
                let targets: Vec<ItemId> = match params.get("ids").and_then(Value::as_array) {
                    Some(ids) => ids.iter().filter_map(Value::as_u64).map(ItemId).collect(),
                    None => state.selection.items.clone(),
                };
                let named = |id: &ItemId| state.doc.item(*id).and_then(|item| item.form_field.as_ref()).is_some_and(|field| !field.name.is_empty());
                Ok((!targets.iter().all(named)).then_some(Self::FormFields))
            }
            _ => Ok(None),
        }
    }

    /// The stored field (as change sets name it) that says whether an item is a member.
    fn field(&self) -> &'static str {
        match self {
            Self::XmlTag(_) => "props.xmlTag",
            Self::FormFields => "props.formField",
            Self::Layer(_) => "layer",
        }
    }

    /// The members now.
    fn now(&self, doc: &kition_layout_engine::doc::Document) -> std::collections::BTreeSet<u64> {
        match self {
            Self::Layer(layer) => doc.layer_items(kition_layout_engine::doc::LayerId(*layer)).into_iter().map(|id| id.0).collect(),
            _ => doc.all_items().into_iter().filter(|id| doc.item(*id).is_some_and(|item| self.holds(item))).map(|id| id.0).collect(),
        }
    }

    fn holds(&self, item: &kition_layout_engine::doc::Item) -> bool {
        match self {
            Self::XmlTag(name) => item.xml_tag == *name,
            Self::FormFields => item.form_field.is_some(),
            Self::Layer(layer) => item.layer.0 == *layer,
        }
    }

    /// Whether an item whose stored values at `basis` were `before` was a member; `None` when
    /// `before` does not tell the field.
    fn held(&self, before: &serde_json::Map<String, Value>) -> Option<bool> {
        match self {
            Self::XmlTag(name) => before.get("props").map(|props| props.get("xmlTag").and_then(Value::as_str) == Some(name.as_str())),
            Self::FormFields => before.get("props").map(|props| props.get("formField").is_some_and(|field| !field.is_null())),
            Self::Layer(layer) => before.get("layer").map(|was| numeric_id(was) == Some(*layer)),
        }
    }
}

impl LayoutDocument {
    pub(super) fn member_premises(&self, members: &Members, under: &mut Under) -> Result<(), String> {
        let doc = &self.active()?.doc;
        for id in members.now(doc) {
            under.objects.push(json!(id));
            under.premises.push(Premise::on(id).fields([members.field()]).positions(Vec::<String>::new()));
        }
        if let Members::XmlTag(name) = members {
            // The tag definition it deletes and the style mappings to the tag it drops.
            let name = serde_json::to_string(name).map_err(crate::engine_error)?;
            under.premises.push(
                Premise::on(ROOT)
                    .fields([format!("settings.xml.tags[name={name}]"), "settings.xml.styleMap".to_owned()])
                    .positions(Vec::<String>::new()),
            );
        }
        under.covers = Some(Covers { fields: vec![members.field().to_owned()] });
        Ok(())
    }

    /// `context.covered` for a set of items: the members at `basis`, with every asked item put
    /// back as it was (a created one was not there; a deleted one is not acted on either way).
    pub(super) fn member_covered(&self, members: &Members, asks: &[CoveredAsk]) -> Result<CoveredAt, String> {
        let doc = &self.active()?.doc;
        let mut then = members.now(doc);
        let mut covered = Vec::with_capacity(asks.len());
        for ask in asks {
            let Some(id) = numeric_id(&ask.id) else {
                covered.push(Covered { id: ask.id.clone(), covered: Some(false), ..Default::default() });
                continue;
            };
            if !ask.unknown.is_empty() && !ask.created {
                return Ok(CoveredAt {
                    under: vec![],
                    covered: asks.iter().map(|ask| Covered { id: ask.id.clone(), covered: None, ..Default::default() }).collect(),
                });
            }
            let was = if ask.created || ask.deleted { false } else { members.held(&ask.before).unwrap_or_else(|| then.contains(&id)) };
            if was {
                then.insert(id);
            } else {
                then.remove(&id);
            }
            covered.push(Covered { id: ask.id.clone(), covered: Some(was), ..Default::default() });
        }
        Ok(CoveredAt { under: then.into_iter().map(|id| json!(id)).collect(), covered })
    }
}
