import type { CreatedInstance } from "../bindings/CreatedInstance";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import type { VersionEntry } from "../bindings/VersionEntry";
import type { AppInfo } from "../bindings/AppInfo";
import type { SettingChange } from "../bindings/SettingChange";
import type { StartupStatus } from "../bindings/StartupStatus";
import type { Settings } from "../bindings/Settings";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { call } from "./ipc";

export const appInfo = () => call<AppInfo>("app_info");
export const getStartupStatus = () => call<StartupStatus>("get_startup_status");
export const getSettings = () => call<Settings>("get_settings");
export const setSetting = (change: SettingChange) => call<Settings>("set_setting", { change });
export const listTasks = () => call<TaskSnapshot[]>("list_tasks");
export const getTask = (taskId: string) => call<TaskSnapshot>("get_task", { taskId });
export const cancelTask = (taskId: string) => call<null>("cancel_task", { taskId });

export const listVersions = () => call<VersionEntry[]>("list_versions");
export const listInstances = () => call<Instance[]>("list_instances");
export const createInstance = (name: string, mcVersion: string) =>
  call<CreatedInstance>("create_instance", { name, mcVersion });
export const launchInstance = (instanceId: string) => call<string>("launch_instance", { instanceId });
export const listRunning = () => call<RunningSession[]>("list_running");
export const getDevOfflineUsername = () => call<string>("get_dev_offline_username");
export const setDevOfflineUsername = (name: string) => call<string>("set_dev_offline_username", { name });
