import type { CreatedInstance } from "../bindings/CreatedInstance";
import type { Instance } from "../bindings/Instance";
import type { InstanceSettings } from "../bindings/InstanceSettings";
import type { RunningSession } from "../bindings/RunningSession";
import type { VersionEntry } from "../bindings/VersionEntry";
import type { AppInfo } from "../bindings/AppInfo";
import type { SettingChange } from "../bindings/SettingChange";
import type { StartupStatus } from "../bindings/StartupStatus";
import type { Settings } from "../bindings/Settings";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import type { AccountList } from "../bindings/AccountList";
import type { FolderKind } from "../bindings/FolderKind";
import type { GameLog } from "../bindings/GameLog";
import type { ImportReport } from "../bindings/ImportReport";
import type { LogContent } from "../bindings/LogContent";
import type { Screenshot } from "../bindings/Screenshot";
import type { LoginMethod } from "../bindings/LoginMethod";
import type { LoaderKind } from "../bindings/LoaderKind";
import type { LoaderVersionEntry } from "../bindings/LoaderVersionEntry";
import type { NewInstance } from "../bindings/NewInstance";
import type { ContentKind } from "../bindings/ContentKind";
import type { ContentItem } from "../bindings/ContentItem";
import type { IdentifyReport } from "../bindings/IdentifyReport";
import type { AddReport } from "../bindings/AddReport";
import type { SearchQuery } from "../bindings/SearchQuery";
import type { SearchPage } from "../bindings/SearchPage";
import type { InstallPreview } from "../bindings/InstallPreview";
import type { UpdateCheck } from "../bindings/UpdateCheck";
import type { ContentTaskReport } from "../bindings/ContentTaskReport";
import type { VersionChoice } from "../bindings/VersionChoice";
import type { Category } from "../bindings/Category";
import type { InstanceSync } from "../bindings/InstanceSync";
import type { JoinMode } from "../bindings/JoinMode";
import type { JoinResult } from "../bindings/JoinResult";
import type { PendingView } from "../bindings/PendingView";
import type { Resolution } from "../bindings/Resolution";
import type { SyncProfile } from "../bindings/SyncProfile";
import type { SyncField } from "../bindings/SyncField";
import type { SyncFieldValue } from "../bindings/SyncFieldValue";
import type { CurrentSkin } from "../bindings/CurrentSkin";
import type { SavedSkin } from "../bindings/SavedSkin";
import type { SavedSkinPatch } from "../bindings/SavedSkinPatch";
import type { DefaultSkin } from "../bindings/DefaultSkin";
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
export const listLoaderGameVersions = (loader: LoaderKind) =>
  call<string[]>("list_loader_game_versions", { loader });
export const listLoaderVersions = (loader: LoaderKind, mcVersion: string) =>
  call<LoaderVersionEntry[]>("list_loader_versions", { loader, mcVersion });
export const createInstance = (request: NewInstance) => call<CreatedInstance>("create_instance", { request });
export const updateInstance = (instanceId: string, settings: InstanceSettings) =>
  call<Instance>("update_instance", { instanceId, settings });
export const deleteInstance = (instanceId: string) => call<null>("delete_instance", { instanceId });
export const duplicateInstance = (instanceId: string) => call<string>("duplicate_instance", { instanceId });
export const launchInstance = (instanceId: string, offline = false) =>
  call<string>("launch_instance", { instanceId, offline });
export const stopInstance = (instanceId: string, force: boolean) =>
  call<null>("stop_instance", { instanceId, force });
export const openInstanceFolder = (instanceId: string, kind: FolderKind) =>
  call<null>("open_instance_folder", { instanceId, kind });
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

export const listContent = (instanceId: string, kind: ContentKind) =>
  call<ContentItem[]>("list_content", { instanceId, kind });
export const identifyContent = (instanceId: string) => call<IdentifyReport>("identify_content", { instanceId });
export const setContentEnabled = (instanceId: string, kind: ContentKind, relPath: string, enabled: boolean) =>
  call<null>("set_content_enabled", { instanceId, kind, relPath, enabled });
export const removeContent = (instanceId: string, kind: ContentKind, relPath: string) =>
  call<null>("remove_content", { instanceId, kind, relPath });
export const addContentFiles = (instanceId: string, kind: ContentKind, paths: string[], overwrite: boolean) =>
  call<AddReport>("add_content_files", { instanceId, kind, paths, overwrite });
export const listContentCategories = (kind: ContentKind) =>
  call<string[]>("list_content_categories", { kind });
export const getContentIcon = (key: string) => call<string | null>("get_content_icon", { key });

export const searchModrinth = (instanceId: string | null, query: SearchQuery, offset: number) =>
  call<SearchPage>("search_modrinth", { instanceId, query, offset });
export const previewContentInstall = (instanceId: string, projectId: string) =>
  call<InstallPreview>("preview_content_install", { instanceId, projectId });
export const installContent = (instanceId: string, projectId: string, acceptIncompatible: boolean) =>
  call<string>("install_content", { instanceId, projectId, acceptIncompatible });
export const checkContentUpdates = (instanceId: string, force: boolean) =>
  call<UpdateCheck>("check_content_updates", { instanceId, force });
export const updateContent = (instanceId: string, relPaths: string[]) =>
  call<string>("update_content", { instanceId, relPaths });
export const listContentVersions = (instanceId: string, relPath: string) =>
  call<VersionChoice[]>("list_content_versions", { instanceId, relPath });
export const changeContentVersion = (instanceId: string, relPath: string, versionId: string) =>
  call<string>("change_content_version", { instanceId, relPath, versionId });
export const contentTaskReport = (taskId: string) =>
  call<ContentTaskReport | null>("content_task_report", { taskId });
export const openModrinthProject = (slug: string) => call<null>("open_modrinth_project", { slug });

export const listSyncProfiles = () => call<SyncProfile[]>("list_sync_profiles");
export const createSyncProfileFromInstance = (instanceId: string, name: string) =>
  call<SyncProfile>("create_sync_profile_from_instance", { instanceId, name });
export const renameSyncProfile = (profileId: string, name: string) =>
  call<null>("rename_sync_profile", { profileId, name });
export const deleteSyncProfile = (profileId: string) => call<null>("delete_sync_profile", { profileId });
export const setSyncCategories = (profileId: string, categories: Category[]) =>
  call<null>("set_sync_categories", { profileId, categories });
export const previewJoinSyncProfile = (instanceId: string, profileId: string) =>
  call<number>("preview_join_sync_profile", { instanceId, profileId });
export const joinSyncProfile = (instanceId: string, profileId: string, mode: JoinMode) =>
  call<JoinResult>("join_sync_profile", { instanceId, profileId, mode });
export const leaveSyncProfile = (instanceId: string) => call<null>("leave_sync_profile", { instanceId });
export const getInstanceSync = (instanceId: string) => call<InstanceSync>("get_instance_sync", { instanceId });
export const setSyncExcluded = (instanceId: string, fieldIds: string[]) =>
  call<null>("set_sync_excluded", { instanceId, fieldIds });
export const listSyncFields = () => call<SyncField[]>("list_sync_fields");
export const getSyncProfileValues = (profileId: string) =>
  call<SyncFieldValue[]>("get_sync_profile_values", { profileId });
export const setSyncOverride = (instanceId: string, fieldId: string, value: string | null) =>
  call<null>("set_sync_override", { instanceId, fieldId, value });
export const pendingSyncProposals = (profileId: string) =>
  call<PendingView>("pending_sync_proposals", { profileId });
export const resolveSyncProposal = (
  instanceId: string,
  fieldId: string,
  resolution: Resolution,
  chosenValue?: string,
) => call<null>("resolve_sync_proposal", { instanceId, fieldId, resolution, chosenValue: chosenValue ?? null });

export const getCurrentSkin = (accountId: string) => call<CurrentSkin>("get_current_skin", { accountId });
export const setCape = (accountId: string, capeId: string | null) => call<null>("set_cape", { accountId, capeId });
export const listSavedSkins = () => call<SavedSkin[]>("list_saved_skins");
export const getSavedSkinImage = (skinId: string) => call<string>("get_saved_skin_image", { skinId });
export const addSavedSkin = (name: string, png: number[]) => call<SavedSkin>("add_saved_skin", { name, png });
export const addSavedSkinFromPath = (path: string) => call<SavedSkin>("add_saved_skin_from_path", { path });
export const updateSavedSkin = (skinId: string, patch: SavedSkinPatch) =>
  call<SavedSkin>("update_saved_skin", { skinId, patch });
export const deleteSavedSkin = (skinId: string) => call<null>("delete_saved_skin", { skinId });
export const applySavedSkin = (accountId: string, skinId: string) =>
  call<null>("apply_saved_skin", { accountId, skinId });
export const importCurrentSkin = (accountId: string) =>
  call<SavedSkin | null>("import_current_skin", { accountId });
export const listDefaultSkins = () => call<DefaultSkin[]>("list_default_skins");
export const applyDefaultSkin = (accountId: string, key: string) =>
  call<null>("apply_default_skin", { accountId, key });

export const listScreenshots = (instanceId: string) =>
  call<Screenshot[]>("list_screenshots", { instanceId });
export const deleteScreenshot = (instanceId: string, name: string) =>
  call<null>("delete_screenshot", { instanceId, name });
export const importScreenshots = (instanceId: string, paths: string[]) =>
  call<ImportReport>("import_screenshots", { instanceId, paths });

export const listGameLogs = (instanceId: string) => call<GameLog[]>("list_game_logs", { instanceId });
export const readGameLog = (instanceId: string, name: string) =>
  call<LogContent>("read_game_log", { instanceId, name });
