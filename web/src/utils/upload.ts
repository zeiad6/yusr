// fetch() reports no upload progress, so archive files go through XMLHttpRequest.
import { ApiError } from "../api/client";
import { useAuthStore } from "../store/auth";

/** crates/yusr-infra/src/archive.rs MAX_FILE_BYTES */
export const MAX_FILE_BYTES = 20 * 1024 * 1024;

export function uploadFile<T>(path: string, file: File, onProgress: (fraction: number) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", `/api${path}`);
    const token = useAuthStore.getState().token;
    if (token) xhr.setRequestHeader("Authorization", `Bearer ${token}`);
    xhr.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total);
    xhr.onload = () => {
      let data: { error?: string; message?: string } | null = null;
      try {
        data = JSON.parse(xhr.responseText);
      } catch {
        // plain-text rejection (body too large before reaching the handler)
      }
      if (xhr.status >= 200 && xhr.status < 300) return resolve(data as T);
      if (xhr.status === 401) useAuthStore.getState().logout();
      const code = xhr.status === 413 ? "file_too_large" : (data?.error ?? "http_error");
      reject(new ApiError(xhr.status, code, data?.message ?? xhr.statusText));
    };
    xhr.onerror = () => reject(new ApiError(0, "network", "network error"));
    const form = new FormData();
    form.append("file", file, file.name);
    xhr.send(form);
  });
}

/** Authenticated download of an archive file as a Blob (the API needs the bearer token). */
export async function fetchBlob(path: string): Promise<Blob> {
  const token = useAuthStore.getState().token;
  const res = await fetch(`/api${path}`, { headers: token ? { Authorization: `Bearer ${token}` } : {} });
  if (!res.ok) throw new ApiError(res.status, res.status === 404 ? "not_found" : "http_error", res.statusText);
  return res.blob();
}
