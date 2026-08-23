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
  relativePath: string;
  parentPath: string;
  isDirectory: boolean;
  size?: number;
};

export type ArtifactDocumentFormat = 'pdf' | 'docx' | 'xlsx' | 'pptx';

export type ArtifactGitStatus = 'untracked' | 'modified' | 'staged' | 'committed' | 'pushed';

export type ArtifactGitEntry = {
  name: string;
  path: string;
  relativePath: string;
  parentPath: string;
  isDirectory: boolean;
  status: ArtifactGitStatus;
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
      size: number;
      found: true;
      // **Exactly one of `dataUrl` and `bytes` is set**, decided by size in the
      // main process. A base64 data URL costs ~4/3 of the file as a JS string
      // and pays it twice — once as an IPC structured clone, once as a DOM
      // attribute — and Chromium degrades on multi-megabyte URLs, so anything
      // large arrives as raw bytes for the renderer to wrap in a `blob:` URL.
      // Read them through `imageSourceForPreview`, never directly, so the
      // blob's lifetime is always someone's job.
      dataUrl?: string;
      bytes?: ArrayBuffer;
    }
  | {
      kind: 'document';
      format: ArtifactDocumentFormat;
      title: string;
      path: string;
      mimeType: string;
      data: ArrayBuffer;
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
      kind: 'gitDirectory';
      title: string;
      path: string;
      branch: string;
      entries: ArtifactGitEntry[];
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
      /** Friendly, human-readable message — never the raw Node errno string. */
      error: string;
      /** Errno-style code ('ENOENT', 'EACCES', …) when one was recognised. */
      code?: string;
      found: false;
    };

export type PreparedArtifactHtml = {
  html: string;
};
