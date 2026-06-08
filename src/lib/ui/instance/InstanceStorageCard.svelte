<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  import {
    controllerFsCapabilitiesGet,
    controllerFsDelete,
    controllerFsList,
    controllerFsMkdir,
    controllerFsPullFile,
    controllerFsPushFile,
    controllerFsRename,
    localFsDelete,
    localFsList,
    localFsMkdir,
    localFsRename,
    pathOpen,
  } from "$lib/api/client";
  import type {
    BridgeInstanceStatus,
    ControllerFsCapabilities,
    ControllerFsFileType,
    ControllerFsListEntry,
    ControllerFsTransferProgressEvent,
    LocalFsEntry,
    LocalFsFileType,
  } from "$lib/api/types";
  import ArrowUpIcon from "$lib/ui/icons/ArrowUpIcon.svelte";
  import DownloadIcon from "$lib/ui/icons/DownloadIcon.svelte";
  import FileIcon from "$lib/ui/icons/FileIcon.svelte";
  import FolderIcon from "$lib/ui/icons/FolderIcon.svelte";
  import FolderPlusIcon from "$lib/ui/icons/FolderPlusIcon.svelte";
  import HomeIcon from "$lib/ui/icons/HomeIcon.svelte";
  import OpenFolderIcon from "$lib/ui/icons/OpenFolderIcon.svelte";
  import RefreshIcon from "$lib/ui/icons/RefreshIcon.svelte";
  import RenameIcon from "$lib/ui/icons/RenameIcon.svelte";
  import TrashIcon from "$lib/ui/icons/TrashIcon.svelte";
  import UploadIcon from "$lib/ui/icons/UploadIcon.svelte";

  export let instance: BridgeInstanceStatus;
  export let disabled = false;

  const DEFAULT_REMOTE_ROOT = "/midi-studio";
  const FALLBACK_REMOTE_ROOT = "/";

  type DraggedItem =
    | { side: "local"; name: string; path: string; fileType: LocalFsFileType }
    | { side: "remote"; name: string; path: string; fileType: ControllerFsFileType };

  type StorageItem =
    | { side: "local"; name: string; path: string; fileType: LocalFsFileType }
    | { side: "remote"; name: string; path: string; fileType: ControllerFsFileType };

  type ContextMenuState = {
    side: "local" | "remote";
    item: StorageItem | null;
    x: number;
    y: number;
  };

  type TransferFile = {
    fromPath: string;
    toPath: string;
    bytesTotal: number;
    transferId: string;
  };

  type TransferProgressState = {
    doneBytes: number;
    totalBytes: number;
    doneFiles: number;
    totalFiles: number;
    fileBytes: Record<string, number>;
  };

  let localPath = "";
  let localRootPath = "";
  let localParentPath: string | null = null;
  let localEntries: LocalFsEntry[] = [];
  let localLoading = false;
  let localError: string | null = null;

  let remotePath = DEFAULT_REMOTE_ROOT;
  let loadedInstanceId: string | null = null;
  let remoteEntries: ControllerFsListEntry[] = [];
  let capabilities: ControllerFsCapabilities | null = null;
  let remoteLoading = false;
  let remoteError: string | null = null;

  let draggedItem: DraggedItem | null = null;
  let transferBusy = false;
  let transferMessage: string | null = null;
  let transferError: string | null = null;
  let transferProgress: TransferProgressState | null = null;
  let transferSequence = 0;
  let localDropActive = false;
  let remoteDropActive = false;
  let remoteFolderDropPath: string | null = null;
  let localSelectedPaths = new Set<string>();
  let remoteSelectedPaths = new Set<string>();
  let localAnchorPath: string | null = null;
  let remoteAnchorPath: string | null = null;
  let contextMenu: ContextMenuState | null = null;

  $: sortedRemoteEntries = [...remoteEntries].sort((a, b) => {
    if (a.file_type === "directory" && b.file_type !== "directory") return -1;
    if (a.file_type !== "directory" && b.file_type === "directory") return 1;
    return a.name.localeCompare(b.name);
  });

  $: canRemoteGoUp = remotePath !== FALLBACK_REMOTE_ROOT;
  $: if (instance.instance_id !== loadedInstanceId) {
    loadedInstanceId = instance.instance_id;
    remotePath = DEFAULT_REMOTE_ROOT;
    remoteEntries = [];
    capabilities = null;
    remoteSelectedPaths = new Set();
    remoteAnchorPath = null;
    contextMenu = null;
    void loadRemote(remotePath, true);
  }

  onMount(() => {
    loadedInstanceId = instance.instance_id;
    void loadLocal(null);
    void loadRemote(remotePath, true);

    const refreshTimer = window.setInterval(() => {
      void refreshVisibleStorage();
    }, 2500);

    let unlistenProgress: (() => void) | null = null;
    void listen<ControllerFsTransferProgressEvent>("controller-fs-transfer-progress", (event) => {
      updateTransferProgress(event.payload);
    }).then((unlisten) => {
      unlistenProgress = unlisten;
    });

    return () => {
      window.clearInterval(refreshTimer);
      unlistenProgress?.();
    };
  });

  function requestBase() {
    return {
      instance_id: instance.instance_id,
      control_port: instance.control_port,
    };
  }

  async function openLocalRoot() {
    if (!localRootPath) return;
    try {
      await pathOpen(localRootPath);
    } catch (e) {
      localError = messageOf(e);
    }
  }

  async function loadLocal(path: string | null) {
    if (localLoading) return;
    localLoading = true;
    localError = null;
    try {
      const result = await localFsList({ path });
      localRootPath = result.root_path;
      localPath = result.path;
      localParentPath = result.parent_path ?? null;
      localEntries = result.entries;
      localSelectedPaths = new Set();
      localAnchorPath = null;
      contextMenu = null;
    } catch (e) {
      localError = messageOf(e);
      localEntries = [];
    } finally {
      localLoading = false;
    }
  }

  async function loadRemote(path: string, loadCapabilities = false) {
    if (remoteLoading || disabled) return;
    remoteLoading = true;
    remoteError = null;
    try {
      if (loadCapabilities && !capabilities) {
        capabilities = await controllerFsCapabilitiesGet(requestBase());
      }
      remoteEntries = await controllerFsList({ ...requestBase(), path });
      remotePath = normalizeRemotePath(path);
      remoteSelectedPaths = new Set();
      remoteAnchorPath = null;
      contextMenu = null;
    } catch (e) {
      remoteError = messageOf(e);
      remoteEntries = [];
    } finally {
      remoteLoading = false;
    }
  }

  async function refreshVisibleStorage() {
    if (transferBusy || contextMenu || draggedItem) return;
    await Promise.allSettled([refreshLocalEntries(), refreshRemoteEntries()]);
  }

  async function refreshLocalEntries() {
    if (localLoading) return;
    try {
      const result = await localFsList({ path: localPath || null });
      localRootPath = result.root_path;
      localPath = result.path;
      localParentPath = result.parent_path ?? null;
      localEntries = result.entries;
      localSelectedPaths = pruneSelection(
        localSelectedPaths,
        result.entries.map((entry) => entry.path),
      );
      if (localAnchorPath && !localSelectedPaths.has(localAnchorPath)) localAnchorPath = null;
      localError = null;
    } catch (e) {
      localError = messageOf(e);
    }
  }

  async function refreshRemoteEntries() {
    if (remoteLoading || disabled) return;
    try {
      remoteEntries = await controllerFsList({ ...requestBase(), path: remotePath });
      remoteSelectedPaths = pruneSelection(
        remoteSelectedPaths,
        remoteEntries.map((entry) => normalizeRemotePath(`${remotePath}/${entry.name}`)),
      );
      if (remoteAnchorPath && !remoteSelectedPaths.has(remoteAnchorPath)) remoteAnchorPath = null;
      remoteError = null;
    } catch (e) {
      remoteError = messageOf(e);
    }
  }

  function pruneSelection(selected: Set<string>, visiblePaths: string[]): Set<string> {
    const visible = new Set(visiblePaths);
    return new Set([...selected].filter((path) => visible.has(path)));
  }

  function normalizeRemotePath(path: string): string {
    if (!path || path === "/") return FALLBACK_REMOTE_ROOT;
    return `/${path.split("/").filter(Boolean).join("/")}`;
  }

  function remoteChildPath(name: string): string {
    return remoteChildPathIn(remotePath, name);
  }

  function remoteChildPathIn(basePath: string, name: string): string {
    const base = basePath === FALLBACK_REMOTE_ROOT ? "" : basePath;
    return normalizeRemotePath(`${base}/${name}`);
  }

  function remoteParentPath(path: string): string {
    const parts = path.split("/").filter(Boolean);
    parts.pop();
    return parts.length ? `/${parts.join("/")}` : FALLBACK_REMOTE_ROOT;
  }

  function localChildPath(name: string): string {
    return localPath === "/" ? `/${name}` : `${localPath}/${name}`;
  }

  function joinLocalPath(basePath: string, name: string): string {
    return basePath === "/" ? `/${name}` : `${basePath}/${name}`;
  }

  function remoteEntryPath(entry: ControllerFsListEntry): string {
    return remoteChildPath(entry.name);
  }

  function localItem(entry: LocalFsEntry): StorageItem {
    return { side: "local", name: entry.name, path: entry.path, fileType: entry.file_type };
  }

  function remoteItem(entry: ControllerFsListEntry): StorageItem {
    return {
      side: "remote",
      name: entry.name,
      path: remoteEntryPath(entry),
      fileType: entry.file_type,
    };
  }

  function midiStudioFileType(name: string): string | null {
    const lower = name.toLowerCase();
    if (lower === "manifest.json") return "manifest";
    if (lower.endsWith(".mspj")) return "project";
    if (lower.endsWith(".mshj")) return "history";
    if (lower.endsWith(".msmacro")) return "macro";
    if (lower.endsWith(".mspattern")) return "pattern";
    if (lower.endsWith(".msscale")) return "scale";
    return null;
  }

  function visibleLocalPaths(): string[] {
    return localEntries.map((entry) => entry.path);
  }

  function visibleRemotePaths(): string[] {
    return sortedRemoteEntries.map((entry) => remoteEntryPath(entry));
  }

  function selectItem(item: StorageItem, event: MouseEvent | null = null) {
    const side = item.side;
    const path = item.path;
    const selected = side === "local" ? localSelectedPaths : remoteSelectedPaths;
    const visiblePaths = side === "local" ? visibleLocalPaths() : visibleRemotePaths();
    const anchor = side === "local" ? localAnchorPath : remoteAnchorPath;
    let next = new Set<string>();

    if (event?.shiftKey && anchor && visiblePaths.includes(anchor)) {
      const start = visiblePaths.indexOf(anchor);
      const end = visiblePaths.indexOf(path);
      if (start >= 0 && end >= 0) {
        const range = visiblePaths.slice(Math.min(start, end), Math.max(start, end) + 1);
        next = event.ctrlKey || event.metaKey ? new Set(selected) : new Set();
        for (const value of range) next.add(value);
      }
    } else if (event?.ctrlKey || event?.metaKey) {
      next = new Set(selected);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
    } else {
      next.add(path);
    }

    if (side === "local") {
      localSelectedPaths = next;
      localAnchorPath = path;
    } else {
      remoteSelectedPaths = next;
      remoteAnchorPath = path;
    }
  }

  function ensureContextSelection(item: StorageItem | null) {
    if (!item) return;
    const selected = item.side === "local" ? localSelectedPaths : remoteSelectedPaths;
    if (selected.has(item.path)) return;
    selectItem(item);
  }

  function openContextMenu(side: "local" | "remote", item: StorageItem | null, event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    ensureContextSelection(item);
    contextMenu = { side, item, x: event.clientX, y: event.clientY };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function localSelection(): StorageItem[] {
    return localEntries
      .filter((entry) => localSelectedPaths.has(entry.path))
      .map(localItem);
  }

  function remoteSelection(): StorageItem[] {
    return sortedRemoteEntries
      .map(remoteItem)
      .filter((entry) => remoteSelectedPaths.has(entry.path));
  }

  function selectedCount(side: "local" | "remote"): number {
    return side === "local" ? localSelectedPaths.size : remoteSelectedPaths.size;
  }

  function remoteParentOf(path: string): string {
    const parts = path.split("/").filter(Boolean);
    parts.pop();
    return parts.length ? `/${parts.join("/")}` : FALLBACK_REMOTE_ROOT;
  }

  function safeChildName(input: string | null): string | null {
    const value = input?.trim();
    if (!value || value.includes("/") || value.includes("\\")) return null;
    return value;
  }

  function handleLocalClick(entry: LocalFsEntry, event: MouseEvent) {
    const item = localItem(entry);
    if (entry.file_type === "directory" && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      void loadLocal(entry.path);
      return;
    }
    selectItem(item, event);
  }

  function handleRemoteClick(entry: ControllerFsListEntry, event: MouseEvent) {
    const item = remoteItem(entry);
    if (entry.file_type === "directory" && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      void loadRemote(remoteEntryPath(entry));
      return;
    }
    selectItem(item, event);
  }

  function startLocalDrag(entry: LocalFsEntry, event: DragEvent) {
    if (entry.file_type === "other" || localLoading || transferBusy) return;
    const item = localItem(entry);
    ensureContextSelection(item);
    setDraggedItem(item, event);
  }

  function startRemoteDrag(entry: ControllerFsListEntry, event: DragEvent) {
    if (entry.file_type !== "file" && entry.file_type !== "directory") return;
    if (disabled || remoteLoading || transferBusy) return;
    const item = remoteItem(entry);
    ensureContextSelection(item);
    setDraggedItem(item, event);
  }

  function setDraggedItem(item: DraggedItem, event: DragEvent) {
    draggedItem = item;
    event.dataTransfer?.setData("application/x-midi-studio-storage-item", JSON.stringify(item));
    event.dataTransfer?.setData("text/plain", item.path);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = item.side === "remote" ? "copyMove" : "copy";
  }

  function allowDrop(side: "local" | "remote", event: DragEvent) {
    const item = draggedItem;
    if (!item || item.side === side) return;

    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    if (side === "local") localDropActive = true;
    if (side === "remote") remoteDropActive = true;
  }

  function leaveDrop(side: "local" | "remote", event: DragEvent) {
    const current = event.currentTarget as HTMLElement;
    const related = event.relatedTarget;
    if (related instanceof Node && current.contains(related)) return;
    if (side === "local") localDropActive = false;
    if (side === "remote") remoteDropActive = false;
  }

  async function dropOn(side: "local" | "remote", event: DragEvent) {
    const item = draggedItemFromEvent(event);
    event.preventDefault();
    clearDrag();
    if (!item || item.side === side) return;

    if (side === "remote" && item.side === "local") {
      const selected = localSelection();
      await uploadLocalItems(selected.length ? selected : [item]);
      return;
    }
    if (side === "local" && item.side === "remote") {
      const selected = remoteSelection();
      await downloadRemoteItems(selected.length ? selected : [item]);
    }
  }

  function draggedItemFromEvent(event: DragEvent): DraggedItem | null {
    if (draggedItem) return draggedItem;

    const raw = event.dataTransfer?.getData("application/x-midi-studio-storage-item");
    if (!raw) return null;

    try {
      const parsed = JSON.parse(raw) as Partial<DraggedItem>;
      if (
        (parsed.side === "local" || parsed.side === "remote") &&
        typeof parsed.name === "string" &&
        typeof parsed.path === "string" &&
        typeof parsed.fileType === "string"
      ) {
        return parsed as DraggedItem;
      }
    } catch {
      // Ignore malformed drag payloads.
    }

    return null;
  }

  function clearDrag() {
    localDropActive = false;
    remoteDropActive = false;
    remoteFolderDropPath = null;
    draggedItem = null;
  }

  function allowRemoteFolderDrop(entry: ControllerFsListEntry, event: DragEvent) {
    const item = draggedItemFromEvent(event);
    const targetPath = remoteEntryPath(entry);
    if (!item || disabled || remoteLoading || transferBusy) return;
    if (item.side === "remote" && !canMoveRemoteItemToFolder(item, targetPath)) return;
    if (item.side === "local" && item.fileType === "other") return;

    event.preventDefault();
    event.stopPropagation();
    remoteFolderDropPath = targetPath;
    if (event.dataTransfer) event.dataTransfer.dropEffect = item.side === "remote" ? "move" : "copy";
  }

  function leaveRemoteFolderDrop(entry: ControllerFsListEntry, event: DragEvent) {
    const current = event.currentTarget as HTMLElement;
    const related = event.relatedTarget;
    if (related instanceof Node && current.contains(related)) return;
    if (remoteFolderDropPath === remoteEntryPath(entry)) remoteFolderDropPath = null;
  }

  async function dropRemoteOnFolder(entry: ControllerFsListEntry, event: DragEvent) {
    const item = draggedItemFromEvent(event);
    const targetPath = remoteEntryPath(entry);
    event.preventDefault();
    event.stopPropagation();
    remoteFolderDropPath = null;
    if (!item) {
      clearDrag();
      return;
    }

    if (item.side === "local") {
      const selectedLocal = localSelection();
      clearDrag();
      await uploadLocalItems(selectedLocal.length ? selectedLocal : [item], targetPath);
      return;
    }

    const selected = remoteSelection();
    clearDrag();
    await moveRemoteItemsToFolder(selected.length ? selected : [item], targetPath);
  }

  function nextTransferId(): string {
    transferSequence += 1;
    return `${Date.now()}-${transferSequence}`;
  }

  function beginTransferProgress(files: TransferFile[]) {
    transferProgress = {
      doneBytes: 0,
      totalBytes: files.reduce((sum, file) => sum + Math.max(0, file.bytesTotal), 0),
      doneFiles: 0,
      totalFiles: files.length,
      fileBytes: {},
    };
  }

  function beginItemProgress(total: number) {
    transferProgress = {
      doneBytes: 0,
      totalBytes: 0,
      doneFiles: 0,
      totalFiles: total,
      fileBytes: {},
    };
  }

  function updateTransferProgress(payload: ControllerFsTransferProgressEvent) {
    if (!transferProgress) return;
    const fileBytes = {
      ...transferProgress.fileBytes,
      [payload.transfer_id]: Math.max(0, payload.bytes_done),
    };
    const doneBytes = Object.values(fileBytes).reduce((sum, bytes) => sum + bytes, 0);
    transferProgress = {
      ...transferProgress,
      fileBytes,
      doneBytes,
      totalBytes: Math.max(transferProgress.totalBytes, payload.bytes_total, doneBytes),
    };
  }

  function completeTransferFile(file: TransferFile) {
    if (!transferProgress) return;
    const completedBytes = Math.max(file.bytesTotal, transferProgress.fileBytes[file.transferId] ?? 0);
    const fileBytes = {
      ...transferProgress.fileBytes,
      [file.transferId]: Math.max(0, completedBytes),
    };
    const doneBytes = Object.values(fileBytes).reduce((sum, bytes) => sum + bytes, 0);
    transferProgress = {
      ...transferProgress,
      fileBytes,
      doneBytes,
      doneFiles: Math.min(transferProgress.doneFiles + 1, transferProgress.totalFiles),
    };
  }

  function completeProgressItem() {
    if (!transferProgress) return;
    transferProgress = {
      ...transferProgress,
      doneFiles: Math.min(transferProgress.doneFiles + 1, transferProgress.totalFiles),
    };
  }

  function progressPercent(progress: TransferProgressState): number {
    if (progress.totalBytes > 0) return Math.min(100, (progress.doneBytes / progress.totalBytes) * 100);
    if (progress.totalFiles > 0) return Math.min(100, (progress.doneFiles / progress.totalFiles) * 100);
    return 0;
  }

  function progressLabel(progress: TransferProgressState): string {
    if (progress.totalBytes > 0) {
      return `${formatSize(progress.doneBytes)} / ${formatSize(progress.totalBytes)} · ${progress.doneFiles}/${progress.totalFiles}`;
    }
    return `${progress.doneFiles}/${progress.totalFiles}`;
  }

  async function uploadLocalItems(items: StorageItem[], targetRemotePath = remotePath) {
    const localItems = items.filter((item) => item.side === "local" && item.fileType !== "other");
    if (!localItems.length || transferBusy || disabled) return;

    transferBusy = true;
    transferError = null;
    transferProgress = null;
    transferMessage = "Preparing upload…";
    try {
      const plan = await collectLocalUploadPlan(localItems, targetRemotePath);
      beginTransferProgress(plan.files);
      for (const dir of plan.directories) {
        await ensureRemoteDirectory(dir);
      }
      for (const file of plan.files) {
        transferMessage = `Uploading ${basename(file.fromPath)}…`;
        await controllerFsPushFile({
          ...requestBase(),
          local_path: file.fromPath,
          remote_path: file.toPath,
          transfer_id: file.transferId,
        });
        completeTransferFile(file);
      }
      transferMessage = `Uploaded ${plan.files.length} file${plan.files.length === 1 ? "" : "s"}`;
      await refreshRemoteEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
      transferProgress = null;
    }
  }

  async function downloadRemoteItems(items: StorageItem[]) {
    const remoteItems = items.filter(
      (item) => item.side === "remote" && (item.fileType === "file" || item.fileType === "directory"),
    );
    if (!remoteItems.length || transferBusy) return;

    transferBusy = true;
    transferError = null;
    transferProgress = null;
    transferMessage = "Preparing download…";
    try {
      const directories: string[] = [];
      const files: TransferFile[] = [];
      for (const item of remoteItems) {
        const entry = remoteEntries.find((candidate) => remoteEntryPath(candidate) === item.path);
        await collectRemoteDownloadFiles(
          item,
          Math.max(0, entry?.size_bytes ?? 0),
          localChildPath(item.name),
          directories,
          files,
        );
      }
      for (const directory of directories) {
        await localFsMkdir({ path: directory });
      }
      beginTransferProgress(files);
      for (const file of files) {
        transferMessage = `Downloading ${basename(file.fromPath)}…`;
        await controllerFsPullFile({
          ...requestBase(),
          remote_path: file.fromPath,
          local_path: file.toPath,
          transfer_id: file.transferId,
        });
        completeTransferFile(file);
      }
      transferMessage = `Downloaded ${files.length} file${files.length === 1 ? "" : "s"}`;
      await refreshLocalEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
      transferProgress = null;
    }
  }

  async function collectLocalUploadPlan(
    items: StorageItem[],
    targetRemotePath: string,
  ): Promise<{
    directories: string[];
    files: TransferFile[];
  }> {
    const directories: string[] = [];
    const files: TransferFile[] = [];
    for (const item of items) {
      if (item.side !== "local") continue;
      const entry = localEntries.find((candidate) => candidate.path === item.path);
      await collectLocalUploadItem(
        item.path,
        item.fileType,
        Math.max(0, entry?.size_bytes ?? 0),
        remoteChildPathIn(targetRemotePath, item.name),
        directories,
        files,
      );
    }
    return { directories, files };
  }

  async function collectLocalUploadItem(
    localItemPath: string,
    fileType: LocalFsFileType,
    bytesTotal: number,
    remoteTargetPath: string,
    directories: string[],
    files: TransferFile[],
  ) {
    if (fileType === "file") {
      files.push({
        fromPath: localItemPath,
        toPath: remoteTargetPath,
        bytesTotal,
        transferId: nextTransferId(),
      });
      return;
    }
    if (fileType !== "directory") return;

    directories.push(remoteTargetPath);
    const listed = await localFsList({ path: localItemPath });
    for (const entry of listed.entries) {
      await collectLocalUploadItem(
        entry.path,
        entry.file_type,
        Math.max(0, entry.size_bytes ?? 0),
        normalizeRemotePath(`${remoteTargetPath}/${entry.name}`),
        directories,
        files,
      );
    }
  }

  async function collectRemoteDownloadFiles(
    item: StorageItem,
    bytesTotal: number,
    localTargetPath: string,
    directories: string[],
    files: TransferFile[],
  ) {
    if (item.side !== "remote") return;
    if (item.fileType === "file") {
      files.push({
        fromPath: item.path,
        toPath: localTargetPath,
        bytesTotal,
        transferId: nextTransferId(),
      });
      return;
    }
    if (item.fileType !== "directory") return;

    directories.push(localTargetPath);
    const entries = await controllerFsList({ ...requestBase(), path: item.path });
    for (const entry of entries) {
      const childPath = normalizeRemotePath(`${item.path}/${entry.name}`);
      await collectRemoteDownloadFiles(
        { side: "remote", name: entry.name, path: childPath, fileType: entry.file_type },
        Math.max(0, entry.size_bytes),
        joinLocalPath(localTargetPath, entry.name),
        directories,
        files,
      );
    }
  }

  async function ensureRemoteDirectory(path: string) {
    try {
      await controllerFsList({ ...requestBase(), path });
      return;
    } catch {
      await controllerFsMkdir({ ...requestBase(), path });
    }
  }

  async function createRemoteFolder() {
    closeContextMenu();
    const name = safeChildName(window.prompt("New folder name"));
    if (!name || transferBusy || disabled) return;
    transferBusy = true;
    transferError = null;
    transferMessage = `Creating ${name}…`;
    try {
      await controllerFsMkdir({ ...requestBase(), path: remoteChildPath(name) });
      transferMessage = `Created ${name}`;
      await loadRemote(remotePath);
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
    }
  }

  async function createLocalFolder() {
    closeContextMenu();
    const name = safeChildName(window.prompt("New folder name"));
    if (!name || transferBusy) return;
    transferBusy = true;
    transferError = null;
    transferMessage = `Creating ${name}…`;
    try {
      await localFsMkdir({ path: joinLocalPath(localPath || "/", name) });
      transferMessage = `Created ${name}`;
      await refreshLocalEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
    }
  }

  async function renameRemoteItem(item: StorageItem | null) {
    closeContextMenu();
    if (!item || item.side !== "remote" || transferBusy || disabled) return;
    const nextName = safeChildName(window.prompt("Rename", item.name));
    if (!nextName || nextName === item.name) return;

    const toPath = normalizeRemotePath(`${remoteParentOf(item.path)}/${nextName}`);
    transferBusy = true;
    transferError = null;
    transferMessage = `Renaming ${item.name}…`;
    try {
      await controllerFsRename({ ...requestBase(), from_path: item.path, to_path: toPath });
      transferMessage = `Renamed ${item.name} -> ${nextName}`;
      await loadRemote(remotePath);
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
    }
  }

  async function renameLocalItem(item: StorageItem | null) {
    closeContextMenu();
    if (!item || item.side !== "local" || transferBusy) return;
    const nextName = safeChildName(window.prompt("Rename", item.name));
    if (!nextName || nextName === item.name) return;

    transferBusy = true;
    transferError = null;
    transferMessage = `Renaming ${item.name}…`;
    try {
      await localFsRename({
        from_path: item.path,
        to_path: joinLocalPath(localParentOf(item.path), nextName),
      });
      transferMessage = `Renamed ${item.name} -> ${nextName}`;
      await refreshLocalEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
    }
  }

  async function deleteRemoteSelection() {
    closeContextMenu();
    const items = remoteSelection();
    if (!items.length || transferBusy || disabled) return;
    const label = items.length === 1 ? items[0].name : `${items.length} items`;
    if (!window.confirm(`Delete ${label} from controller storage?`)) return;

    transferBusy = true;
    transferError = null;
    beginItemProgress(items.length);
    try {
      for (const item of items) {
        transferMessage = `Deleting ${item.name}…`;
        await controllerFsDelete({
          ...requestBase(),
          path: item.path,
          recursive: item.fileType === "directory",
        });
        completeProgressItem();
      }
      transferMessage = `Deleted ${label}`;
      await refreshRemoteEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
      transferProgress = null;
    }
  }

  async function deleteLocalSelection() {
    closeContextMenu();
    const items = localSelection();
    if (!items.length || transferBusy) return;
    const label = items.length === 1 ? items[0].name : `${items.length} items`;
    if (!window.confirm(`Delete ${label} from PC storage?`)) return;

    transferBusy = true;
    transferError = null;
    beginItemProgress(items.length);
    try {
      for (const item of items) {
        transferMessage = `Deleting ${item.name}…`;
        await localFsDelete({
          path: item.path,
          recursive: item.fileType === "directory",
        });
        completeProgressItem();
      }
      transferMessage = `Deleted ${label}`;
      await refreshLocalEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
      transferProgress = null;
    }
  }

  async function moveRemoteItemsToFolder(items: StorageItem[], targetFolderPath: string) {
    const remoteItems = items.filter((item) => item.side === "remote");
    if (!remoteItems.length || transferBusy || disabled) return;
    const movable = remoteItems.filter((item) => canMoveRemoteItemToFolder(item, targetFolderPath));
    if (!movable.length) return;

    transferBusy = true;
    transferError = null;
    beginItemProgress(movable.length);
    try {
      for (const item of movable) {
        transferMessage = `Moving ${item.name}…`;
        await controllerFsRename({
          ...requestBase(),
          from_path: item.path,
          to_path: normalizeRemotePath(`${targetFolderPath}/${item.name}`),
        });
        completeProgressItem();
      }
      transferMessage = `Moved ${movable.length} item${movable.length === 1 ? "" : "s"}`;
      await refreshRemoteEntries();
    } catch (e) {
      transferError = messageOf(e);
    } finally {
      transferBusy = false;
      transferProgress = null;
    }
  }

  function canMoveRemoteItemToFolder(item: StorageItem | DraggedItem, targetFolderPath: string): boolean {
    if (item.side !== "remote" || item.fileType === "missing" || item.fileType === "other") return false;
    const targetPath = normalizeRemotePath(targetFolderPath);
    if (item.path === targetPath) return false;
    if (item.fileType === "directory" && targetPath.startsWith(`${item.path}/`)) return false;
    return normalizeRemotePath(`${targetPath}/${item.name}`) !== item.path;
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }

  function localParentOf(path: string): string {
    const parts = path.split("/").filter(Boolean);
    parts.pop();
    return parts.length ? `/${parts.join("/")}` : "/";
  }

  function formatSize(bytes: number | null | undefined): string {
    if (bytes == null) return "-";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  }

  function messageOf(value: unknown): string {
    const err = value as { message?: string };
    return typeof err?.message === "string" ? err.message : String(value);
  }
</script>

<svelte:window onclick={closeContextMenu} onkeydown={(event) => event.key === "Escape" && closeContextMenu()} />

<section class="storageCard">
  <div class="storageHeader">
    <div>
      <div class="title">Storage</div>
      <div class="detail">
        {#if capabilities}
          Chunk {capabilities.max_chunk_size} B · {capabilities.max_list_entries} entries/page
        {:else}
          PC/controller transfer
        {/if}
      </div>
    </div>
    {#if transferMessage || transferError}
      <div class:errorText={!!transferError} class="transferState">{transferError ?? transferMessage}</div>
    {/if}
  </div>

  {#if transferProgress}
    <div class="progress" aria-label="Transfer progress">
      <div
        class="progressFill"
        style:width={`${progressPercent(transferProgress)}%`}
      ></div>
      <span>{progressLabel(transferProgress)}</span>
    </div>
  {/if}

  <div class="panes">
    <section
      class="pane"
      class:dropActive={localDropActive}
      data-storage-drop-side="local"
      aria-label="Local PC storage drop zone"
      ondragenter={(event) => allowDrop("local", event)}
      ondragover={(event) => allowDrop("local", event)}
      ondragleave={(event) => leaveDrop("local", event)}
      ondrop={(event) => dropOn("local", event)}
      oncontextmenu={(event) => openContextMenu("local", null, event)}
    >
      <div class="paneHeader">
        <div>
          <div class="paneTitle">PC</div>
          <div class="pathBar">{localPath || "-"}</div>
          <div class="rootHint">MIDI Studio exchange root</div>
        </div>
        <div class="actions">
          <button class="iconButton" type="button" disabled={localLoading || transferBusy} onclick={createLocalFolder} title="New folder" aria-label="New folder">
            <FolderPlusIcon size={15} />
          </button>
          <button class="iconButton" type="button" disabled={localLoading || transferBusy} onclick={() => loadLocal(localPath)} title="Refresh" aria-label="Refresh PC folder">
            <RefreshIcon size={15} />
          </button>
          <button class="iconButton" type="button" disabled={localLoading || transferBusy || !localRootPath} onclick={openLocalRoot} title="Open PC folder" aria-label="Open PC folder">
            <OpenFolderIcon size={15} />
          </button>
        </div>
      </div>

      {#if localError}
        <div class="err">{localError}</div>
      {:else}
        <div class="table" aria-busy={localLoading}>
          <div class="head name">Name</div>
          <div class="head type">Type</div>
          <div class="head size">Size</div>
          {#if localLoading}
            <div class="empty">Loading PC folder…</div>
          {:else}
            {#if localParentPath}
              <button
                class="row name clickable parentRow"
                type="button"
                disabled={localLoading || transferBusy}
                onclick={() => localParentPath && loadLocal(localParentPath)}
              >
                <span class="folderIcon" aria-hidden="true"><ArrowUpIcon size={14} /></span>
                <span class="entryName">..</span>
              </button>
              <div class="cell type">parent</div>
              <div class="cell size">-</div>
            {/if}
          {/if}
          {#if !localLoading && localEntries.length === 0}
            <div class="empty">No entries.</div>
          {:else if !localLoading}
            {#each localEntries as entry (entry.path)}
              {#if entry.file_type === "directory"}
                <button
                  class="row name clickable"
                  class:selected={localSelectedPaths.has(entry.path)}
                  type="button"
                  disabled={localLoading || transferBusy}
                  draggable={!localLoading && !transferBusy}
                  onclick={(event) => handleLocalClick(entry, event)}
                  ondragstart={(event) => startLocalDrag(entry, event)}
                  ondragend={clearDrag}
                  oncontextmenu={(event) => openContextMenu("local", localItem(entry), event)}
                >
                  <span class="folderIcon" aria-hidden="true"><FolderIcon size={14} /></span>
                  <span class="entryName">{entry.name}</span>
                </button>
              {:else}
                {@const fileKind = midiStudioFileType(entry.name)}
                <button
                  class="row name"
                  class:selected={localSelectedPaths.has(entry.path)}
                  class:draggableFile={entry.file_type === "file"}
                  type="button"
                  draggable={entry.file_type === "file" && !localLoading && !transferBusy}
                  disabled={localLoading || transferBusy}
                  onclick={(event) => selectItem(localItem(entry), event)}
                  ondragstart={(event) => startLocalDrag(entry, event)}
                  ondragend={clearDrag}
                  oncontextmenu={(event) => openContextMenu("local", localItem(entry), event)}
                >
                  <span class="fileIcon" aria-hidden="true"><FileIcon size={14} /></span>
                  <span class="entryName">{entry.name}</span>
                  {#if fileKind}
                    <span class="fileBadge">{fileKind}</span>
                  {/if}
                </button>
              {/if}
              <div class="cell type">{entry.file_type}</div>
              <div class="cell size">{formatSize(entry.size_bytes)}</div>
            {/each}
          {/if}
        </div>
      {/if}
    </section>

    <section
      class="pane"
      class:dropActive={remoteDropActive}
      data-storage-drop-side="remote"
      aria-label="Controller storage drop zone"
      ondragenter={(event) => allowDrop("remote", event)}
      ondragover={(event) => allowDrop("remote", event)}
      ondragleave={(event) => leaveDrop("remote", event)}
      ondrop={(event) => dropOn("remote", event)}
      oncontextmenu={(event) => openContextMenu("remote", null, event)}
    >
      <div class="paneHeader">
        <div>
          <div class="paneTitle">Controller</div>
          <div class="pathBar">{remotePath}</div>
        </div>
        <div class="actions">
          <button class="iconButton" type="button" disabled={disabled || remoteLoading || transferBusy} onclick={createRemoteFolder} title="New folder" aria-label="New folder">
            <FolderPlusIcon size={15} />
          </button>
          <button class="iconButton" type="button" disabled={disabled || remoteLoading || transferBusy} onclick={() => loadRemote(remotePath)} title="Refresh" aria-label="Refresh controller folder">
            <RefreshIcon size={15} />
          </button>
          <button class="iconButton" type="button" disabled={disabled || remoteLoading || transferBusy || remotePath === FALLBACK_REMOTE_ROOT} onclick={() => loadRemote(FALLBACK_REMOTE_ROOT)} title="Root" aria-label="Go to controller root">
            <HomeIcon size={15} />
          </button>
        </div>
      </div>

      {#if remoteError}
        <div class="err">
          <div>{remoteError}</div>
          {#if remotePath !== FALLBACK_REMOTE_ROOT}
            <button class="linkButton" type="button" disabled={disabled || remoteLoading} onclick={() => loadRemote(FALLBACK_REMOTE_ROOT, true)}>
              Try root
            </button>
          {/if}
        </div>
      {:else}
        <div class="table" aria-busy={remoteLoading}>
          <div class="head name">Name</div>
          <div class="head type">Type</div>
          <div class="head size">Size</div>
          {#if remoteLoading}
            <div class="empty">Loading controller…</div>
          {:else}
            {#if canRemoteGoUp}
              <button
                class="row name clickable parentRow"
                type="button"
                disabled={disabled || remoteLoading || transferBusy}
                onclick={() => loadRemote(remoteParentPath(remotePath))}
              >
                <span class="folderIcon" aria-hidden="true"><ArrowUpIcon size={14} /></span>
                <span class="entryName">..</span>
              </button>
              <div class="cell type">parent</div>
              <div class="cell size">-</div>
            {/if}
          {/if}
          {#if !remoteLoading && sortedRemoteEntries.length === 0}
            <div class="empty">No entries.</div>
          {:else if !remoteLoading}
            {#each sortedRemoteEntries as entry (`${entry.file_type}:${entry.name}`)}
              {#if entry.file_type === "directory"}
                <button
                  class="row name clickable"
                  class:selected={remoteSelectedPaths.has(remoteEntryPath(entry))}
                  class:dropTarget={remoteFolderDropPath === remoteEntryPath(entry)}
                  type="button"
                  disabled={disabled || remoteLoading || transferBusy}
                  draggable={!disabled && !remoteLoading && !transferBusy}
                  onclick={(event) => handleRemoteClick(entry, event)}
                  ondragstart={(event) => startRemoteDrag(entry, event)}
                  ondragenter={(event) => allowRemoteFolderDrop(entry, event)}
                  ondragover={(event) => allowRemoteFolderDrop(entry, event)}
                  ondragleave={(event) => leaveRemoteFolderDrop(entry, event)}
                  ondrop={(event) => dropRemoteOnFolder(entry, event)}
                  ondragend={clearDrag}
                  oncontextmenu={(event) => openContextMenu("remote", remoteItem(entry), event)}
                >
                  <span class="folderIcon" aria-hidden="true"><FolderIcon size={14} /></span>
                  <span class="entryName">{entry.name}</span>
                  {#if entry.name_truncated}
                    <span class="truncated">truncated</span>
                  {/if}
                </button>
              {:else}
                {@const fileKind = midiStudioFileType(entry.name)}
                <button
                  class="row name"
                  class:selected={remoteSelectedPaths.has(remoteEntryPath(entry))}
                  class:draggableFile={entry.file_type === "file"}
                  type="button"
                  draggable={entry.file_type === "file" && !disabled && !remoteLoading && !transferBusy}
                  disabled={disabled || remoteLoading || transferBusy}
                  onclick={(event) => selectItem(remoteItem(entry), event)}
                  ondragstart={(event) => startRemoteDrag(entry, event)}
                  ondragend={clearDrag}
                  oncontextmenu={(event) => openContextMenu("remote", remoteItem(entry), event)}
                >
                  <span class="fileIcon" aria-hidden="true"><FileIcon size={14} /></span>
                  <span class="entryName">{entry.name}</span>
                  {#if fileKind}
                    <span class="fileBadge">{fileKind}</span>
                  {/if}
                  {#if entry.name_truncated}
                    <span class="truncated">truncated</span>
                  {/if}
                </button>
              {/if}
              <div class="cell type">{entry.file_type}</div>
              <div class="cell size">{entry.file_type === "file" ? formatSize(entry.size_bytes) : "-"}</div>
            {/each}
          {/if}
        </div>
      {/if}
    </section>
  </div>

  {#if contextMenu}
    <div
      class="contextMenu"
      style:left={`${contextMenu.x}px`}
      style:top={`${contextMenu.y}px`}
      role="menu"
      tabindex="-1"
      oncontextmenu={(event) => event.preventDefault()}
    >
      {#if contextMenu.side === "local"}
        <button type="button" role="menuitem" disabled={selectedCount("local") === 0 || transferBusy || disabled} onclick={() => { closeContextMenu(); void uploadLocalItems(localSelection()); }}>
          <span class="menuIcon" aria-hidden="true"><UploadIcon size={14} /></span>
          <span>Upload selected ({selectedCount("local")})</span>
        </button>
        <button type="button" role="menuitem" disabled={!contextMenu.item || selectedCount("local") !== 1 || transferBusy} onclick={() => renameLocalItem(contextMenu?.item ?? null)}>
          <span class="menuIcon" aria-hidden="true"><RenameIcon size={14} /></span>
          <span>Rename</span>
        </button>
        <button type="button" role="menuitem" disabled={selectedCount("local") === 0 || transferBusy} onclick={deleteLocalSelection}>
          <span class="menuIcon" aria-hidden="true"><TrashIcon size={14} /></span>
          <span>Delete selected ({selectedCount("local")})</span>
        </button>
        <div class="menuDivider"></div>
        <button type="button" role="menuitem" disabled={transferBusy} onclick={createLocalFolder}>
          <span class="menuIcon" aria-hidden="true"><FolderPlusIcon size={14} /></span>
          <span>New folder</span>
        </button>
        <button type="button" role="menuitem" disabled={!localRootPath} onclick={openLocalRoot}>
          <span class="menuIcon" aria-hidden="true"><OpenFolderIcon size={14} /></span>
          <span>Open PC root</span>
        </button>
      {:else}
        <button type="button" role="menuitem" disabled={selectedCount("remote") === 0 || transferBusy} onclick={() => { closeContextMenu(); void downloadRemoteItems(remoteSelection()); }}>
          <span class="menuIcon" aria-hidden="true"><DownloadIcon size={14} /></span>
          <span>Download selected ({selectedCount("remote")})</span>
        </button>
        <button type="button" role="menuitem" disabled={!contextMenu.item || selectedCount("remote") !== 1 || transferBusy} onclick={() => renameRemoteItem(contextMenu?.item ?? null)}>
          <span class="menuIcon" aria-hidden="true"><RenameIcon size={14} /></span>
          <span>Rename</span>
        </button>
        <button type="button" role="menuitem" disabled={selectedCount("remote") === 0 || transferBusy} onclick={deleteRemoteSelection}>
          <span class="menuIcon" aria-hidden="true"><TrashIcon size={14} /></span>
          <span>Delete selected ({selectedCount("remote")})</span>
        </button>
        <div class="menuDivider"></div>
        <button type="button" role="menuitem" disabled={transferBusy || disabled} onclick={createRemoteFolder}>
          <span class="menuIcon" aria-hidden="true"><FolderPlusIcon size={14} /></span>
          <span>New folder</span>
        </button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .storageCard {
    display: grid;
    gap: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-3) var(--space-4);
    background: color-mix(in srgb, var(--panel) 70%, transparent);
  }

  .storageHeader,
  .paneHeader {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: var(--space-4);
  }

  .title,
  .paneTitle {
    color: var(--fg);
    font-size: 13px;
    line-height: 16px;
    font-weight: 700;
  }

  .detail,
  .transferState {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .transferState {
    max-width: 48%;
    text-align: right;
    overflow-wrap: anywhere;
  }

  .transferState.errorText {
    color: var(--err);
  }

  .panes {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-3);
  }

  .pane {
    display: grid;
    align-content: start;
    gap: var(--space-3);
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    padding: var(--space-3);
    background: color-mix(in srgb, var(--bg) 34%, transparent);
  }

  .pane.dropActive {
    border-color: var(--value);
    background: color-mix(in srgb, var(--value) 10%, var(--bg));
  }

  .progress {
    position: relative;
    height: 18px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    background: color-mix(in srgb, var(--bg) 40%, transparent);
  }

  .progressFill {
    position: absolute;
    inset: 0 auto 0 0;
    width: 0;
    background: color-mix(in srgb, var(--value) 44%, transparent);
    transition: width 120ms ease;
  }

  .progress span {
    position: relative;
    z-index: 1;
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--fg);
    font-size: 11px;
    line-height: 14px;
    font-weight: 800;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .iconButton {
    appearance: none;
    font: inherit;
    width: var(--control-height);
    height: var(--control-height);
    padding: 0;
    border-radius: var(--control-radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    display: inline-grid;
    place-items: center;
  }

  .iconButton:hover:not(:disabled) {
    color: var(--fg);
    background: color-mix(in srgb, var(--value) 11%, transparent);
  }

  .iconButton:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .pathBar {
    color: var(--fg);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
    margin-top: 3px;
  }

  .rootHint {
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
    margin-top: 2px;
  }

  .table {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 78px 76px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    overflow: hidden;
  }

  .head,
  .cell,
  .row {
    min-height: 32px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    line-height: 16px;
  }

  .head {
    color: var(--muted);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    background: color-mix(in srgb, var(--bg) 48%, transparent);
  }

  .row {
    appearance: none;
    border-top: 0;
    border-right: 0;
    border-left: 0;
    background: transparent;
    color: var(--fg);
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    cursor: default;
    user-select: none;
  }

  .row.clickable,
  .row.draggableFile {
    cursor: pointer;
  }

  .row.draggableFile {
    color: color-mix(in srgb, var(--fg) 84%, var(--value));
  }

  .row.clickable:hover,
  .row.draggableFile:hover {
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .row.parentRow {
    color: var(--muted);
    font-weight: 800;
  }

  .row.selected {
    background: color-mix(in srgb, var(--value) 18%, transparent);
    color: var(--fg);
  }

  .row.dropTarget {
    background: color-mix(in srgb, var(--value) 26%, transparent);
    outline: 1px solid color-mix(in srgb, var(--value) 72%, transparent);
    outline-offset: -2px;
  }

  .row:disabled {
    opacity: 1;
  }

  .cell {
    color: var(--muted);
    display: flex;
    align-items: center;
  }

  .size {
    justify-content: flex-end;
    text-align: right;
  }

  .folderIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: var(--value);
    flex: 0 0 auto;
  }

  .fileIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: color-mix(in srgb, var(--muted) 84%, var(--fg) 16%);
    flex: 0 0 auto;
  }

  .entryName {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fileBadge {
    flex: 0 0 auto;
    border: 1px solid color-mix(in srgb, var(--value) 28%, var(--border));
    border-radius: 999px;
    padding: 1px 5px;
    color: color-mix(in srgb, var(--value) 72%, var(--fg));
    background: color-mix(in srgb, var(--value) 8%, transparent);
    font-size: 10px;
    line-height: 12px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .truncated {
    color: var(--warn);
    font-size: 11px;
    flex: 0 0 auto;
  }

  .empty {
    grid-column: 1 / -1;
    min-height: 48px;
    display: grid;
    place-items: center;
    color: var(--muted);
    font-size: 12px;
  }

  .err {
    color: var(--err);
    font-size: 12px;
    line-height: 16px;
    border: 1px solid var(--err);
    border-radius: var(--control-radius);
    padding: var(--space-3) var(--space-4);
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: center;
  }

  .linkButton {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--err);
    cursor: pointer;
    font: inherit;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
  }

  .contextMenu {
    position: fixed;
    z-index: 1000;
    min-width: 184px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    padding: 4px;
    background: var(--panel);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--fg) 8%, transparent),
      0 12px 32px rgba(0, 0, 0, 0.42);
  }

  .contextMenu button {
    appearance: none;
    width: 100%;
    min-height: 30px;
    border: 0;
    border-radius: calc(var(--control-radius) - 2px);
    background: transparent;
    color: var(--fg);
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    line-height: 16px;
    text-align: left;
    padding: 6px 9px;
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .contextMenu button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--value) 14%, transparent);
  }

  .contextMenu button:disabled {
    color: var(--muted);
    cursor: not-allowed;
    opacity: 0.58;
  }

  .menuIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: currentColor;
    flex: 0 0 auto;
  }

  .menuDivider {
    height: 1px;
    margin: 4px 2px;
    background: var(--border);
  }

  @media (max-width: 960px) {
    .panes {
      grid-template-columns: 1fr;
    }

    .transferState {
      max-width: none;
      text-align: left;
    }
  }
</style>
