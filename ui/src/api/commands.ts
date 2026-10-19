import type { CreatedInstance } from "../bindings/CreatedInstance";
import type { Instance } from "../bindings/Instance";
import type { InstanceSettings } from "../bindings/InstanceSettings";
import type { SkinVariant } from "../bindings/SkinVariant";
import type { RunningSession } from "../bindings/RunningSession";
import type { VersionEntry } from "../bindings/VersionEntry";
import type { AppInfo } from "../bindings/AppInfo";
import type { SettingChange } from "../bindings/SettingChange";
import type { StartupStatus } from "../bindings/StartupStatus";
import type { Settings } from "../bindings/Settings";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import type { AccountList } from "../bindings/AccountList";
import type { LoginMethod } from "../bindings/LoginMethod";
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
export const updateInstance = (instanceId: string, settings: InstanceSettings) =>
  call<Instance>("update_instance", { instanceId, settings });
export const deleteInstance = (instanceId: string) => call<null>("delete_instance", { instanceId });
export const duplicateInstance = (instanceId: string) => call<string>("duplicate_instance", { instanceId });
export const launchInstance = (instanceId: string, offline = false) =>
  call<string>("launch_instance", { instanceId, offline });
export const listRunning = () => call<RunningSession[]>("list_running");
export const systemMemoryMb = () => call<number>("system_memory_mb");
export const getDevOfflineUsername = () => call<string>("get_dev_offline_username");
export const setDevOfflineUsername = (name: string) => call<string>("set_dev_offline_username", { name });

export const listAccounts = () => call<AccountList>("list_accounts");
export const setActiveAccount = (accountId: string) => call<null>("set_active_account", { accountId });
export const setDevOfflineActive = (active: boolean) => call<null>("set_dev_offline_active", { active });
export const removeAccount = (accountId: string) => call<null>("remove_account", { accountId });
export const beginLogin = (method: LoginMethod) => call<string>("begin_login", { method });
export const getSkin = (accountId: string) => call<string | null>("get_skin", { accountId });
export const getSkinVariant = (accountId: string) => call<SkinVariant>("get_skin_variant", { accountId });
export const changeSkin = (accountId: string, variant: SkinVariant, png: number[]) =>
  call<null>("change_skin", { accountId, variant, png });
