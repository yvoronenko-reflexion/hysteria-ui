import { invoke } from "@tauri-apps/api/core";
import type { HysteriaConfig, Profile, StatusPayload, VersionInfo } from "../types";

export const importUri = (uri: string) => invoke<Profile>("import_uri", { uri });
export const exportUri = (profile: Profile) => invoke<string>("export_uri", { profile });
export const generateYaml = (config: HysteriaConfig, mode: string) =>
  invoke<string>("generate_yaml", { config, mode });

export const listProfiles = () => invoke<Profile[]>("list_profiles");
export const saveProfile = (profile: Profile) => invoke<Profile[]>("save_profile", { profile });
export const deleteProfile = (id: string) => invoke<Profile[]>("delete_profile", { id });

export const connect = (profile: Profile, mode: string) =>
  invoke<void>("connect", { profile, mode });
export const disconnect = () => invoke<void>("disconnect");
export const connectionStatus = () => invoke<StatusPayload>("connection_status");
/** Version + path of the bundled hysteria binary. */
export const hysteriaVersion = () => invoke<VersionInfo>("hysteria_version");
/** Externally visible IP — via the local proxy listener in proxy mode, direct otherwise. */
export const externalIp = () => invoke<string>("external_ip");

export const privilegeStatus = () => invoke<boolean>("privilege_status");
export const installPrivileges = () => invoke<void>("install_privileges");
export const uninstallPrivileges = () => invoke<void>("uninstall_privileges");
