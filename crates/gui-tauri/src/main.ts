import { getFormats, listArchive, testArchive, cancelTask, type FormatInfo } from "./bridge";

// --------------- Format list display ---------------

window.addEventListener("DOMContentLoaded", async () => {
  try {
    const formats = await getFormats();

    const list = document.getElementById("format-list")!;
    const loading = document.getElementById("loading")!;

    for (const fmt of formats) {
      const row = document.createElement("tr");

      const nameCell = document.createElement("td");
      nameCell.textContent = fmt.name;
      row.appendChild(nameCell);

      const compressCell = document.createElement("td");
      compressCell.textContent = fmt.can_compress ? "Yes" : "No";
      compressCell.className = fmt.can_compress ? "supported" : "unsupported";
      row.appendChild(compressCell);

      const decompressCell = document.createElement("td");
      decompressCell.textContent = fmt.can_decompress ? "Yes" : "No";
      decompressCell.className = fmt.can_decompress ? "supported" : "unsupported";
      row.appendChild(decompressCell);

      list.appendChild(row);
    }

    loading.textContent = `${formats.length} formats loaded`;
  } catch (e) {
    document.getElementById("loading")!.textContent =
      `Failed to load formats: ${e}`;
  }
});

// --------------- Dev preview: bridge test ---------------

/** Run a quick test from the browser DevTools console.
 *
 * Usage:
 *   bridgeTest("/path/to/archive.zip")
 *   bridgeTest("/path/to/file.gz")
 */
(window as unknown as Record<string, unknown>).bridgeTest = async (
  path: string,
) => {
  console.log("=== GeeZipX Bridge Test ===");
  console.log("Archive:", path);

  try {
    const entries = await listArchive(path);
    console.log(`Entries (${entries.length}):`, entries);
  } catch (e) {
    console.warn("list_archive failed:", e);
  }

  try {
    const result = await testArchive(path);
    console.log("Test result:", result);
  } catch (e) {
    console.warn("test_archive failed:", e);
  }

  console.log("=== Bridge Test Complete ===");
};
