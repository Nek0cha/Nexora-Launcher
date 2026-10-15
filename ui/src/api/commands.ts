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
export const startDemoTask = (failAt?: number) => call<string>("start_demo_task", { failAt: failAt ?? null });
