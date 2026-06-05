import { invoke } from "@tauri-apps/api/core";

interface FormatInfo {
  name: string;
  can_compress: boolean;
  can_decompress: boolean;
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    const formats = await invoke<FormatInfo[]>("get_formats");

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
