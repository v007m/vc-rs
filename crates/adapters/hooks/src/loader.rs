//! How a hook's `import`s are found: `vibranium:std`, or a relative path to another of the
//! call's modules. Nothing else can be imported (D172).

use std::rc::Rc;

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error, Module};

use vc_model::import_paths;

use crate::state::CallState;
use crate::stdlib::{STD, Std};

/// Resolves and loads imports from the running call's modules.
#[derive(Clone)]
pub struct HookLoader(Rc<CallState>);

impl HookLoader {
    /// A loader over `state`'s modules.
    pub const fn new(state: Rc<CallState>) -> Self {
        Self(state)
    }
}

impl Resolver for HookLoader {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if name == STD {
            return Ok(STD.to_owned());
        }
        if !(name.starts_with("./") || name.starts_with("../")) {
            return Err(Error::new_resolving_message(
                base,
                name,
                "only `vibranium:std` and relative paths can be imported",
            ));
        }
        join(base, name)
            .and_then(|path| import_paths(&path).into_iter().find(|candidate| self.0.has_module(candidate)))
            .ok_or_else(|| Error::new_resolving(base, name))
    }
}

impl Loader for HookLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if name == STD {
            return Module::declare_def::<Std, _>(ctx.clone(), STD);
        }
        let js = self.0.module_js(name).ok_or_else(|| Error::new_loading(name))?;
        Module::declare(ctx.clone(), name, js)
    }
}

/// `name` (relative) from the folder of `base`, both project-relative; `None` when it climbs out
/// of the project.
fn join(base: &str, name: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.rsplit_once('/').map_or_else(Vec::new, |(folder, _)| folder.split('/').collect());
    for part in name.split('/').filter(|part| !part.is_empty() && *part != ".") {
        if part == ".." {
            parts.pop()?;
        } else {
            parts.push(part);
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::join;

    #[test]
    fn joins_a_relative_import_to_the_importing_folder() {
        assert_eq!(join("hooks/sign.ts", "./util.ts").as_deref(), Some("hooks/util.ts"));
        assert_eq!(join("hooks/deep/sign.ts", "../lib/./util").as_deref(), Some("hooks/lib/util"));
        assert_eq!(join("sign.ts", "./util.ts").as_deref(), Some("util.ts"));
        assert_eq!(join("hooks/sign.ts", "../../out.ts"), None, "it cannot leave the project");
    }
}
