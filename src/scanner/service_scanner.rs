use super::leftovers::{Integration, command_target, owns};
use crate::{
    uninstall::elevation::ComApartment,
    utils::{paths::is_local_absolute, strings::expand},
};
use std::path::{Path, PathBuf};
use windows_api::{
    Win32::System::{
        Com::{CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CoCreateInstance},
        TaskScheduler::{IExecAction, ITaskFolder, ITaskService, TASK_ENUM_HIDDEN, TaskScheduler},
        Variant::VARIANT,
    },
    core::{BSTR, Interface},
};
use windows_registry::LOCAL_MACHINE;

pub(crate) const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

pub(crate) fn scan(root: &Path, findings: &mut Vec<Integration>) {
    if let Ok(_apartment) = ComApartment::open()
        && let Ok(service) = task_service()
        && let Ok(folder) = unsafe { service.GetFolder(&BSTR::from(r"\")) }
    {
        collect_tasks(&folder, root, 0, &mut 2_000, findings);
    }
    if let Ok(services) = LOCAL_MACHINE.open(SERVICES)
        && let Ok(keys) = services.keys()
    {
        findings.extend(keys.take(4_000).filter_map(|name| {
            let key = services.open(&name).ok()?;
            let kind = key.get_u32("Type").ok()?;
            let command = key.get_string("ImagePath").ok()?;
            let target = command_target(&command)?;
            (kind & 0x30 != 0 && owns(root, &target)).then_some(Integration::Service {
                name,
                command,
                target,
            })
        }));
    }
}

fn collect_tasks(
    folder: &ITaskFolder,
    root: &Path,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<Integration>,
) {
    if depth >= 16 || *budget == 0 {
        return;
    }
    let folder_path = unsafe { folder.Path() }
        .map(|path| path.to_string())
        .unwrap_or_default();
    if folder_path
        .to_ascii_lowercase()
        .starts_with(r"\microsoft\windows")
    {
        return;
    }
    if let Ok(tasks) = unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) }
        && let Ok(count) = unsafe { tasks.Count() }
    {
        for index in 1..=count {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            if let Ok(task) = unsafe { tasks.get_Item(&VARIANT::from(index)) }
                && let Ok(targets) = task_targets(&task)
                && !targets.is_empty()
                && targets
                    .iter()
                    .all(|target| is_local_absolute(target) && owns(root, target))
                && let Ok(path) = unsafe { task.Path() }
            {
                out.push(Integration::Task {
                    path: path.to_string(),
                    targets,
                });
            }
        }
    }
    if let Ok(folders) = unsafe { folder.GetFolders(0) }
        && let Ok(count) = unsafe { folders.Count() }
    {
        for index in 1..=count.min(500) {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            if let Ok(child) = unsafe { folders.get_Item(&VARIANT::from(index)) } {
                collect_tasks(&child, root, depth + 1, budget, out);
            }
        }
    }
}

pub(crate) fn task_service() -> windows_api::core::Result<ITaskService> {
    let service: ITaskService = unsafe {
        CoCreateInstance(
            &TaskScheduler,
            None,
            CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
        )?
    };
    let empty = VARIANT::default();
    unsafe { service.Connect(&empty, &empty, &empty, &empty)? };
    Ok(service)
}

pub(crate) fn task_targets(
    task: &windows_api::Win32::System::TaskScheduler::IRegisteredTask,
) -> windows_api::core::Result<Vec<PathBuf>> {
    let actions = unsafe { task.Definition()?.Actions()? };
    let mut count = 0;
    unsafe { actions.Count(&mut count)? };
    let mut targets = Vec::new();
    for index in 1..=count.min(32) {
        let action: IExecAction = unsafe { actions.get_Item(index)? }.cast()?;
        let mut path = BSTR::new();
        unsafe { action.Path(&mut path)? };
        let expanded = expand(&path.to_string()).unwrap_or_default();
        targets.push(PathBuf::from(expanded));
    }
    Ok(targets)
}
