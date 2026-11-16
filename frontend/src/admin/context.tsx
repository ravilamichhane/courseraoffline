import {
	Alert,
	Button,
	Paper,
	PasswordInput,
	Stack,
	Text,
	Title,
} from "@mantine/core";
import {
	BookOpenTextIcon,
	KeyIcon,
	SignInIcon,
	WarningCircleIcon,
} from "@phosphor-icons/react";
import {
	createContext,
	type FormEvent,
	type ReactNode,
	useCallback,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";
import type { GetDashboardResponse } from "../gen/proto/admin_pb";
import {
	adminClient,
	clearApiKey,
	errorMessage,
	getApiKey,
	isUnauthenticatedError,
	setApiKey,
} from "./api";

type Notice = { message: string; error: boolean };
type AdminContext = {
	data: GetDashboardResponse | null;
	loading: boolean;
	loadError: string | null;
	notice: Notice | null;
	dismiss: () => void;
	refresh: () => Promise<void>;
	run: (action: () => Promise<unknown>, message: string) => Promise<boolean>;
	logout: () => void;
};

const Context = createContext<AdminContext | null>(null);

function isWorkActive(d: GetDashboardResponse | null): boolean {
	if (!d) return false;
	const hasActiveJobs = d.downloads.some((j) =>
		["planning", "queued", "running"].includes(j.state),
	);
	const hasSyncingCourses = d.courses.some(
		(c) => c.metadataState === "syncing" || c.outlineState === "syncing",
	);
	return hasActiveJobs || hasSyncingCourses;
}

function AuthScreen({
	initialError,
	onAuthenticated,
}: {
	initialError: string | null;
	onAuthenticated: (result: GetDashboardResponse) => void;
}) {
	const [keyInput, setKeyInput] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(initialError);

	useEffect(() => {
		if (initialError) setError(initialError);
	}, [initialError]);

	const handleSubmit = async (e: FormEvent) => {
		e.preventDefault();
		const trimmed = keyInput.trim();
		if (!trimmed) {
			setError("Please enter your API key.");
			return;
		}
		setSubmitting(true);
		setError(null);
		setApiKey(trimmed);
		try {
			const result = await adminClient.getDashboard({}, { timeoutMs: 30000 });
			onAuthenticated(result);
		} catch (err) {
			if (isUnauthenticatedError(err)) {
				clearApiKey();
				setError("Invalid API key. Please check and try again.");
			} else {
				setError(errorMessage(err));
			}
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div className="auth-screen">
			<Paper withBorder className="auth-card" p="xl">
				<form onSubmit={handleSubmit}>
					<Stack gap="lg">
						<div className="auth-header">
							<span className="library-brand-mark auth-brand-mark">
								<BookOpenTextIcon size={22} weight="duotone" />
							</span>
							<div>
								<p className="learner-eyebrow auth-eyebrow">
									PERSONAL LEARNING SPACE
								</p>
								<Title order={2} size="h3" className="auth-title">
									courseviewer
								</Title>
								<Text size="xs" c="dimmed" mt={4}>
									Enter your API key to unlock your library
								</Text>
							</div>
						</div>

						{error && (
							<Alert color="red" icon={<WarningCircleIcon size={16} />} py="xs">
								<Text size="xs">{error}</Text>
							</Alert>
						)}

						<PasswordInput
							label="API key"
							placeholder="Enter API key…"
							leftSection={<KeyIcon size={15} />}
							value={keyInput}
							onChange={(e) => setKeyInput(e.currentTarget.value)}
							disabled={submitting}
							autoFocus
							required
						/>

						<Button
							type="submit"
							fullWidth
							size="md"
							loading={submitting}
							leftSection={<SignInIcon size={17} />}
						>
							Unlock library
						</Button>
					</Stack>
				</form>
			</Paper>
		</div>
	);
}

export function AdminProvider({ children }: { children: ReactNode }) {
	const [data, setData] = useState<GetDashboardResponse | null>(null);
	const [loading, setLoading] = useState(() => Boolean(getApiKey()));
	const [loadError, setLoadError] = useState<string | null>(null);
	const [authRequired, setAuthRequired] = useState(() => !getApiKey());
	const [authError, setAuthError] = useState<string | null>(null);
	const [notice, setNotice] = useState<Notice | null>(null);
	const sequence = useRef(0);

	const inFlightPromiseRef =
		useRef<Promise<GetDashboardResponse | null> | null>(null);
	const pollTimerRef = useRef<number | null>(null);
	const dataRef = useRef<GetDashboardResponse | null>(data);
	dataRef.current = data;
	const authRequiredRef = useRef(authRequired);
	authRequiredRef.current = authRequired;
	const lastFetchedRef = useRef(0);
	const hasInitialDataRef = useRef(false);
	const mountedRef = useRef(true);

	const refresh = useCallback(async (): Promise<void> => {
		if (!getApiKey()) {
			setAuthRequired(true);
			setLoading(false);
			return;
		}

		// Deduplicate: if a request is already running, wait for it instead of spawning duplicate calls
		if (inFlightPromiseRef.current) {
			await inFlightPromiseRef.current;
			return;
		}

		const request = ++sequence.current;
		const fetchPromise = (async () => {
			try {
				const result = await adminClient.getDashboard({}, { timeoutMs: 30000 });
				if (request === sequence.current && mountedRef.current) {
					setData(result);
					setLoadError(null);
					setAuthRequired(false);
					setAuthError(null);
					lastFetchedRef.current = Date.now();
				}
				return result;
			} catch (error) {
				if (request === sequence.current && mountedRef.current) {
					if (isUnauthenticatedError(error)) {
						clearApiKey();
						setData(null);
						setAuthRequired(true);
						setAuthError("Invalid or expired API key.");
					} else {
						setLoadError(errorMessage(error));
					}
				}
				return null;
			} finally {
				if (request === sequence.current && mountedRef.current) {
					setLoading(false);
				}
				inFlightPromiseRef.current = null;
			}
		})();

		inFlightPromiseRef.current = fetchPromise;
		await fetchPromise;
	}, []);

	const scheduleNextPoll = useCallback(
		(delayMs: number) => {
			if (pollTimerRef.current) {
				window.clearTimeout(pollTimerRef.current);
				pollTimerRef.current = null;
			}
			if (!mountedRef.current || authRequiredRef.current || document.hidden) {
				return;
			}
			pollTimerRef.current = window.setTimeout(async () => {
				pollTimerRef.current = null;
				if (!mountedRef.current || authRequiredRef.current || document.hidden) {
					return;
				}
				await refresh();
				if (
					mountedRef.current &&
					!authRequiredRef.current &&
					!document.hidden
				) {
					const nextDelay = isWorkActive(dataRef.current) ? 4000 : 30000;
					scheduleNextPoll(nextDelay);
				}
			}, delayMs);
		},
		[refresh],
	);

	useEffect(() => {
		mountedRef.current = true;
		if (authRequired) return;

		if (hasInitialDataRef.current) {
			hasInitialDataRef.current = false;
			const delay = isWorkActive(dataRef.current) ? 4000 : 30000;
			scheduleNextPoll(delay);
		} else {
			void refresh().then(() => {
				const delay = isWorkActive(dataRef.current) ? 4000 : 30000;
				scheduleNextPoll(delay);
			});
		}

		const handleVisibilityChange = () => {
			if (document.hidden) {
				if (pollTimerRef.current) {
					window.clearTimeout(pollTimerRef.current);
					pollTimerRef.current = null;
				}
			} else {
				if (!authRequiredRef.current) {
					const isStale = Date.now() - lastFetchedRef.current > 10000;
					if (isStale) {
						void refresh().then(() => {
							const delay = isWorkActive(dataRef.current) ? 4000 : 30000;
							scheduleNextPoll(delay);
						});
					} else {
						const delay = isWorkActive(dataRef.current) ? 4000 : 30000;
						scheduleNextPoll(delay);
					}
				}
			}
		};

		document.addEventListener("visibilitychange", handleVisibilityChange);

		return () => {
			mountedRef.current = false;
			document.removeEventListener("visibilitychange", handleVisibilityChange);
			if (pollTimerRef.current) {
				window.clearTimeout(pollTimerRef.current);
				pollTimerRef.current = null;
			}
			++sequence.current;
		};
	}, [authRequired, refresh, scheduleNextPoll]);

	useEffect(() => {
		if (!notice) return;
		const timer = setTimeout(
			() => setNotice(null),
			notice.error ? 10000 : 4000,
		);
		return () => clearTimeout(timer);
	}, [notice]);

	const run = async (action: () => Promise<unknown>, message: string) => {
		try {
			await action();
			setNotice({ message, error: false });
			await refresh();
			scheduleNextPoll(isWorkActive(dataRef.current) ? 4000 : 30000);
			return true;
		} catch (error) {
			if (isUnauthenticatedError(error)) {
				clearApiKey();
				setData(null);
				setAuthRequired(true);
				setAuthError("Authentication required. Please re-enter your API key.");
				return false;
			}
			setNotice({ message: errorMessage(error), error: true });
			return false;
		}
	};

	const logout = useCallback(() => {
		++sequence.current;
		if (pollTimerRef.current) {
			window.clearTimeout(pollTimerRef.current);
			pollTimerRef.current = null;
		}
		inFlightPromiseRef.current = null;
		clearApiKey();
		setData(null);
		setLoadError(null);
		setAuthError(null);
		setAuthRequired(true);
		setLoading(false);
	}, []);

	if (authRequired) {
		return (
			<AuthScreen
				initialError={authError}
				onAuthenticated={(result) => {
					setData(result);
					hasInitialDataRef.current = true;
					lastFetchedRef.current = Date.now();
					setLoadError(null);
					setAuthError(null);
					setAuthRequired(false);
					setLoading(false);
					if (
						typeof window !== "undefined" &&
						window.location.pathname !== "/"
					) {
						window.history.replaceState(null, "", "/");
						window.dispatchEvent(new PopStateEvent("popstate"));
					}
				}}
			/>
		);
	}

	return (
		<Context.Provider
			value={{
				data,
				loading,
				loadError,
				notice,
				dismiss: () => setNotice(null),
				refresh,
				run,
				logout,
			}}
		>
			{children}
		</Context.Provider>
	);
}

export function useAdmin() {
	const context = useContext(Context);
	if (!context) throw new Error("AdminProvider is required");
	return context;
}
