export const PHOTO_VIEW_MODES = [
  { id: "comfortable", label: "Comfortable", title: "Large thumbnails" },
  { id: "compact", label: "Compact", title: "Medium thumbnails" },
  { id: "dense", label: "Dense", title: "Small thumbnails" },
  { id: "list", label: "List", title: "Details list" },
] as const;

export type PhotoViewMode = (typeof PHOTO_VIEW_MODES)[number]["id"];

const GRID_CLASSES: Record<PhotoViewMode, string> = {
  comfortable: "grid-cols-[repeat(auto-fill,minmax(172px,1fr))] gap-2.5",
  compact: "grid-cols-[repeat(auto-fill,minmax(128px,1fr))] gap-2",
  dense: "grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-1.5",
  list: "grid-cols-1 gap-1.5",
};

const TILE_CLASSES: Record<PhotoViewMode, string> = {
  comfortable: "aspect-square rounded-lg bg-muted [contain-intrinsic-size:200px] hover:-translate-y-0.5",
  compact: "aspect-square rounded-md bg-muted [contain-intrinsic-size:148px] hover:-translate-y-0.5",
  dense: "aspect-square rounded-md bg-muted [contain-intrinsic-size:112px]",
  list: "flex min-h-[76px] items-stretch gap-3 rounded-lg bg-card p-2 [contain-intrinsic-size:76px]",
};

const SKELETON_CLASSES: Record<PhotoViewMode, string> = {
  comfortable: "aspect-square rounded-lg",
  compact: "aspect-square rounded-md",
  dense: "aspect-square rounded-md",
  list: "h-[76px] rounded-lg",
};

export function isPhotoViewMode(value: string | null | undefined): value is PhotoViewMode {
  return PHOTO_VIEW_MODES.some((mode) => mode.id === value);
}

export function galleryGridClass(mode: PhotoViewMode): string {
  return GRID_CLASSES[mode];
}

export function galleryTileClass(mode: PhotoViewMode): string {
  return TILE_CLASSES[mode];
}

export function gallerySkeletonClass(mode: PhotoViewMode): string {
  return SKELETON_CLASSES[mode];
}
