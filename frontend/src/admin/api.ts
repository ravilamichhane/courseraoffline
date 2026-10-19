import { Code, ConnectError, createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { AdminService } from "../gen/proto/admin_pb";

const API_KEY_STORAGE_KEY = "courseviewer_api_key";

function syncApiKeyCookie(key: string) {
	if (typeof document === "undefined") return;
	const trimmed = key.trim();
	if (trimmed) {
		// biome-ignore lint/suspicious/noDocumentCookie: cookie authenticates native <video> and <track> media requests
		document.cookie = `${API_KEY_STORAGE_KEY}=${encodeURIComponent(trimmed)}; Path=/; Max-Age=31536000; SameSite=Lax`;
	} else {
		// biome-ignore lint/suspicious/noDocumentCookie: clear authentication cookie on logout
		document.cookie = `${API_KEY_STORAGE_KEY}=; Path=/; Max-Age=0; SameSite=Lax`;
	}
}

export function getApiKey(): string {
	if (typeof window === "undefined") return "";
	try {
		return (window.localStorage.getItem(API_KEY_STORAGE_KEY) || "").trim();
	} catch {
		return "";
	}
}

export function setApiKey(key: string): void {
	const trimmed = key.trim();
	if (typeof window !== "undefined") {
		try {
			if (trimmed) {
				window.localStorage.setItem(API_KEY_STORAGE_KEY, trimmed);
			} else {
				window.localStorage.removeItem(API_KEY_STORAGE_KEY);
			}
		} catch {
			// Ignore storage errors
		}
	}
	syncApiKeyCookie(trimmed);
}

export function clearApiKey(): void {
	setApiKey("");
}

if (typeof window !== "undefined") {
	const initialKey = getApiKey();
	if (initialKey) {
		syncApiKeyCookie(initialKey);
	}
}

export function mediaUrl(relativePath: string): string {
	const key = getApiKey();
	const base = `/local-media/${relativePath}`;
	if (!key) return base;
	return `${base}?api_key=${encodeURIComponent(key)}`;
}

export function isUnauthenticatedError(error: unknown): boolean {
	return error instanceof ConnectError && error.code === Code.Unauthenticated;
}

export const adminClient = createClient(
	AdminService,
	createConnectTransport({
		baseUrl: import.meta.env.VITE_RPC_BASE_URL || window.location.origin,
		interceptors: [
			(next) => async (req) => {
				const key = getApiKey();
				if (key) {
					req.header.set("x-api-key", key);
					req.header.set("authorization", `Bearer ${key}`);
				}
				return await next(req);
			},
		],
	}),
);

export function errorMessage(error: unknown) {
	if (error instanceof ConnectError) return error.rawMessage;
	return error instanceof Error
		? error.message
		: "Something went wrong. Please try again.";
}

export function bytes(value: bigint | number) {
	const n = Number(value);
	if (!n) return "0 B";
	const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), 4);
	return `${(n / 1024 ** i).toLocaleString(undefined, { maximumFractionDigits: 1 })} ${["B", "KB", "MB", "GB", "TB"][i]}`;
}

export function date(value: bigint) {
	return new Date(Number(value)).toLocaleDateString(undefined, {
		month: "short",
		day: "numeric",
		year: "numeric",
	});
}

export function slugify(value: string) {
	return value
		.toLowerCase()
		.normalize("NFKD")
		.replace(/[\u0300-\u036f]/g, "")
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-|-$/g, "");
}
