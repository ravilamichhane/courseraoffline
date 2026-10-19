import {
	Alert,
	Button,
	Group,
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
			const result = await adminClient.getDashboard({}, { timeoutMs: 15000 });
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
					<Stack gap="md">
						<Group gap="xs" align="center">
							<span className="course-tile">
								<BookOpenTextIcon size={18} weight="duotone" />
							</span>
							<div>
								<Title order={2} size="h4">
									courseviewer
								</Title>
								<Text size="xs" c="dimmed">
									Enter your API key to access the library
								</Text>
							</div>
						</Group>

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
							loading={submitting}
							leftSection={<SignInIcon size={16} />}
						>
							Unlock
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

	const refresh = useCallback(async () => {
		if (!getApiKey()) {
			setAuthRequired(true);
			setLoading(false);
			return;
		}
		const request = ++sequence.current;
		try {
			const result = await adminClient.getDashboard({}, { timeoutMs: 15000 });
			if (request === sequence.current) {
				setData(result);
				setLoadError(null);
				setAuthRequired(false);
				setAuthError(null);
			}
		} catch (error) {
			if (request === sequence.current) {
				if (isUnauthenticatedError(error)) {
					clearApiKey();
					setData(null);
					setAuthRequired(true);
					setAuthError("Invalid or expired API key.");
				} else {
					setLoadError(errorMessage(error));
				}
			}
		} finally {
			if (request === sequence.current) setLoading(false);
		}
	}, []);

	useEffect(() => {
		if (authRequired) return;
		void refresh();
		const timer = window.setInterval(() => {
			if (!document.hidden && !authRequired) void refresh();
		}, 10000);
		return () => {
			clearInterval(timer);
			++sequence.current;
		};
	}, [refresh, authRequired]);

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
					setLoadError(null);
					setAuthError(null);
					setAuthRequired(false);
					setLoading(false);
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
