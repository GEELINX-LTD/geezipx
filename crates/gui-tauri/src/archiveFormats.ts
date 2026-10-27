// Known archive suffixes used for routing and file dialogs. Native capabilities
// still determine which operations the current build can perform.
// Keep compound suffixes first so output suggestions strip the entire suffix.
export const ARCHIVE_EXTENSIONS = [
  'tar.gz', 'tar.bz2', 'tar.br', 'tar.lz4', 'tar.xz', 'tar.zst',
  'tgz', 'tbz', 'tbz2', 'txz', 'tzst',
  'zip', 'zipx', 'jar', 'war', 'apk', 'ipa', 'xpi', 'tar', 'gz', 'gzip',
  'bz2', 'br', 'lz4', 'xz', 'zst', 'zstd', 'lzma', '7z', 'rar', 'cab',
  'asar', 'deb', 'lzh', 'lha', 'cpio', 'iso', 'zpaq', 'udf', 'wim', 'isz',
  'arj', 'ace', 'arc', 'alz', 'z', 'uu', 'uue', 'xxe', 'img', 'ima', 'bin',
  'enc', 'lz',
];

function archiveSuffix(filePath: string): string | undefined {
  const name = filePath.split(/[/\\]/).pop()?.toLowerCase() ?? '';
  return ARCHIVE_EXTENSIONS.find(ext => name.endsWith(`.${ext}`));
}

export function isArchivePath(filePath: string): boolean {
  return archiveSuffix(filePath) !== undefined;
}

export function stripArchiveExtension(fileName: string): string {
  const ext = archiveSuffix(fileName);
  return ext ? fileName.slice(0, -ext.length - 1) : fileName;
}
