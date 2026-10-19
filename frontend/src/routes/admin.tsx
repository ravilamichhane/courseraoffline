import {
	ActionIcon,
	Alert,
	AppShell,
	Burger,
	Button,
	Group,
	Loader,
	Notification,
	Stack,
	Text,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import {
	ArrowClockwiseIcon,
	ArrowLeftIcon,
	BookOpenTextIcon,
	BooksIcon,
	ChartPieSliceIcon,
	CheckIcon,
	DownloadSimpleIcon,
	GearSixIcon,
	SignOutIcon,
	SquaresFourIcon,
	WarningCircleIcon,
	XIcon,
} from "@phosphor-icons/react";
import {
	createFileRoute,
	Link,
	Outlet,
	useRouterState,
} from "@tanstack/react-router";
import { useAdmin } from "../admin/context";

export const Route = createFileRoute("/admin")({ component: AdminShell });

const nav = [
	{ to: "/admin", label: "Overview", icon: ChartPieSliceIcon },
	{ to: "/admin/courses", label: "Courses", icon: BooksIcon },
	{
		to: "/admin/specializations",
		label: "Specialisations",
		icon: SquaresFourIcon,
	},
	{ to: "/admin/downloads", label: "Downloads", icon: DownloadSimpleIcon },
	{ to: "/admin/settings", label: "Configuration", icon: GearSixIcon },
] as const;

function AdminShell() {
	const [opened, { toggle, close }] = useDisclosure();
	const pathname = useRouterState({ select: (s) => s.location.pathname });
	const { data, loading, loadError, notice, dismiss, refresh, logout } =
		useAdmin();
	const current =
		nav.find((item) => item.to === pathname.replace(/\/$/, "")) || nav[0];
	const activeJobs =
		data?.downloads.filter((j) =>
			["running", "queued", "planning"].includes(j.state),
		).length || 0;

	return (
		<AppShell
			layout="alt"
			header={{ height: 60 }}
			navbar={{ width: 232, breakpoint: "sm", collapsed: { mobile: !opened } }}
			padding={0}
		>
			<AppShell.Navbar className="admin-sidebar">
				<Link to="/" className="brand" onClick={close}>
					<BookOpenTextIcon size={20} weight="duotone" />
					<span>courseviewer</span>
					<span className="admin-label">Admin</span>
				</Link>
				<nav aria-label="Admin navigation" className="admin-nav">
					{nav.map(({ to, label, icon: Icon }) => (
						<Link
							key={to}
							to={to}
							onClick={close}
							className={`nav-item ${current.to === to ? "active" : ""}`}
							aria-current={current.to === to ? "page" : undefined}
						>
							<Icon size={18} weight={current.to === to ? "fill" : "regular"} />
							<span>{label}</span>
							{to === "/admin/downloads" && activeJobs > 0 && (
								<span className="nav-count">{activeJobs}</span>
							)}
						</Link>
					))}
				</nav>
				<div className="local-status">
					<span className={`status-light ${loadError ? "offline" : ""}`} />
					<Text size="xs" c="dimmed">
						{loadError ? "Offline" : loading ? "Connecting…" : "Connected"}
					</Text>
				</div>
			</AppShell.Navbar>

			<AppShell.Header className="admin-header">
				<Group gap={8}>
					<Burger
						opened={opened}
						onClick={toggle}
						hiddenFrom="sm"
						size="sm"
						aria-label="Toggle navigation"
					/>
					<Text size="xs" c="dimmed">
						Admin
					</Text>
					<Text size="xs" c="dimmed">
						/
					</Text>
					<Text size="xs" fw={500}>
						{current.label}
					</Text>
				</Group>
				<Group gap="xs">
					<Button
						component={Link}
						to="/"
						variant="default"
						size="compact-xs"
						leftSection={<ArrowLeftIcon size={13} />}
					>
						Library
					</Button>
					<ActionIcon
						variant="subtle"
						color="gray"
						aria-label="Refresh dashboard"
						onClick={() => void refresh()}
					>
						<ArrowClockwiseIcon size={17} />
					</ActionIcon>
					<ActionIcon
						variant="subtle"
						color="gray"
						aria-label="Sign out"
						title="Sign out"
						onClick={logout}
					>
						<SignOutIcon size={17} />
					</ActionIcon>
				</Group>
			</AppShell.Header>

			<AppShell.Main>
				<div className="admin-content">
					{loadError && (
						<Alert
							color="red"
							title="Connection error"
							icon={<WarningCircleIcon size={18} />}
							mb="lg"
						>
							<Group justify="space-between" align="center" gap="sm">
								<Text size="xs">{loadError}</Text>
								<Button
									size="compact-xs"
									variant="light"
									color="red"
									onClick={() => void refresh()}
								>
									Retry
								</Button>
							</Group>
						</Alert>
					)}
					{loading && !data ? (
						<Stack align="center" py={80} gap="xs">
							<Loader size="sm" />
							<Text c="dimmed" size="xs">
								Loading…
							</Text>
						</Stack>
					) : data ? (
						<Outlet />
					) : null}
				</div>
			</AppShell.Main>

			{notice && (
				<Notification
					className="admin-notice"
					color={notice.error ? "red" : "teal"}
					icon={notice.error ? <XIcon size={16} /> : <CheckIcon size={16} />}
					title={notice.error ? "Error" : "Saved"}
					onClose={dismiss}
					withBorder
				>
					{notice.message}
				</Notification>
			)}
		</AppShell>
	);
}
