export interface BrxtEnvVar {
  key: string;
  required: boolean;
  auto_propagate: boolean;
  default?: string;
  description: string;
  secret: boolean;
}

export interface BrxtManifest {
  name: string;
  display_name: string;
  description: string;
  version: string;
  entry_point: string;
  repository: string;
  tools_count?: number;
  env_vars: BrxtEnvVar[];
}
