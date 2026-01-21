import { app } from 'electron';
import fs from 'fs';
import path from 'path';

export interface EnvToggles {
  BIOROUTER_SERVER__MEMORY: boolean;
  BIOROUTER_SERVER__COMPUTER_CONTROLLER: boolean;
}

export interface ExternalBiorouterdConfig {
  enabled: boolean;
  url: string;
  secret: string;
}

export interface Settings {
  envToggles: EnvToggles;
  showMenuBarIcon: boolean;
  showDockIcon: boolean;
  enableWakelock: boolean;
  spellcheckEnabled: boolean;
  externalBiorouterd?: ExternalBiorouterdConfig;
}

const SETTINGS_FILE = path.join(app.getPath('userData'), 'settings.json');

const defaultSettings: Settings = {
  envToggles: {
    BIOROUTER_SERVER__MEMORY: false,
    BIOROUTER_SERVER__COMPUTER_CONTROLLER: false,
  },
  showMenuBarIcon: true,
  showDockIcon: true,
  enableWakelock: false,
  spellcheckEnabled: true,
};

// Settings management
export function loadSettings(): Settings {
  try {
    if (fs.existsSync(SETTINGS_FILE)) {
      const data = fs.readFileSync(SETTINGS_FILE, 'utf8');
      return JSON.parse(data);
    }
  } catch (error) {
    console.error('Error loading settings:', error);
  }
  return defaultSettings;
}

export function saveSettings(settings: Settings): void {
  try {
    fs.writeFileSync(SETTINGS_FILE, JSON.stringify(settings, null, 2));
  } catch (error) {
    console.error('Error saving settings:', error);
  }
}

export function updateEnvironmentVariables(envToggles: EnvToggles): void {
  if (envToggles.BIOROUTER_SERVER__MEMORY) {
    process.env.BIOROUTER_SERVER__MEMORY = 'true';
  } else {
    delete process.env.BIOROUTER_SERVER__MEMORY;
  }

  if (envToggles.BIOROUTER_SERVER__COMPUTER_CONTROLLER) {
    process.env.BIOROUTER_SERVER__COMPUTER_CONTROLLER = 'true';
  } else {
    delete process.env.BIOROUTER_SERVER__COMPUTER_CONTROLLER;
  }
}
