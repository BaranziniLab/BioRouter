import type { ResourceContents } from '../../api';

export type ArtifactSource =
  | {
      kind: 'html';
      title: string;
      html: string;
      preferredWidth?: number;
      preferredHeight?: number;
    }
  | {
      kind: 'externalUrl';
      title: string;
      url: string;
    }
  | {
      kind: 'file';
      title: string;
      path: string;
    }
  | {
      kind: 'mcpResource';
      title: string;
      resource: ResourceContents;
      preferredWidth?: number;
      preferredHeight?: number;
    };

export type ArtifactFileEntry = {
  name: string;
  path: string;
  isDirectory: boolean;
  size?: number;
};

export type ArtifactFilePreview =
  | {
      kind: 'text' | 'html';
      title: string;
      path: string;
      mimeType: string;
      text: string;
      size: number;
      found: true;
      // For an HTML file, the security-prepared (asset-inlined) HTML the rendered
      // Preview toggle shows. `text` stays the raw source the Raw toggle shows.
      // Only ArtifactViewer sets this (it calls prepareArtifactHtml); the main
      // process read handler does not.
      preparedHtml?: string;
    }
  | {
      kind: 'image';
      title: string;
      path: string;
      mimeType: string;
      dataUrl: string;
      size: number;
      found: true;
    }
  | {
      kind: 'directory';
      title: string;
      path: string;
      entries: ArtifactFileEntry[];
      found: true;
    }
  | {
      kind: 'binary';
      title: string;
      path: string;
      mimeType: string;
      size: number;
      found: true;
    }
  | {
      kind: 'error';
      title: string;
      path: string;
      error: string;
      found: false;
    };

export type PreparedArtifactHtml = {
  html: string;
};
