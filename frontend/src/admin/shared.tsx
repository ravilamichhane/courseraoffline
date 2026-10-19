import { Badge, Button, Group, Paper, Text, Title } from "@mantine/core";
import { ArrowRightIcon, BooksIcon, type Icon } from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

export function PageHeading({
	title,
	action,
}: {
	title: string;
	action?: ReactNode;
}) {
	return (
		<div className="page-heading">
			<Title order={1}>{title}</Title>
			{action}
		</div>
	);
}

export function EmptyState({
	title,
	action,
	icon: Icon = BooksIcon,
}: {
	title: string;
	action?: ReactNode;
	icon?: Icon;
}) {
	return (
		<div className="empty-state">
			<div className="empty-icon">
				<Icon size={24} weight="duotone" />
			</div>
			<Title order={3}>{title}</Title>
			{action && <div className="empty-action">{action}</div>}
		</div>
	);
}

export function StatusBadge({ state }: { state: string }) {
	const color =
		(
			{
				ready: "teal",
				completed: "teal",
				downloaded: "teal",
				running: "blue",
				downloading: "blue",
				queued: "orange",
				planning: "orange",
				partial: "yellow",
				remaining: "gray",
				paused: "gray",
				summary: "gray",
				failed: "red",
				partial_failure: "red",
				cancelled: "gray",
				archived: "gray",
				not_fetched: "gray",
				skipped: "gray",
			} as Record<string, string>
		)[state] || "gray";
	const label =
		(
			{
				summary: "Imported",
				not_fetched: "Not fetched",
				partial_failure: "Partial failure",
				running: "Downloading",
				downloading: "Downloading",
				downloaded: "Downloaded",
				remaining: "Remaining",
				partial: "Partial",
				skipped: "Optional",
			} as Record<string, string>
		)[state] || state.replaceAll("_", " ");
	return (
		<Badge
			color={color}
			variant="light"
			radius="sm"
			size="sm"
			className="status-badge"
		>
			<span className="status-dot" />
			{label}
		</Badge>
	);
}

export function StatCard({
	title,
	value,
	icon: Icon,
	to,
}: {
	title: string;
	value: string | number;
	icon: Icon;
	color?: string;
	to?: string;
}) {
	const content = (
		<>
			<Group justify="space-between" align="center" wrap="nowrap" gap="xs">
				<Text size="xs" c="dimmed" fw={500}>
					{title}
				</Text>
				<span className="stat-icon">
					<Icon size={18} weight="duotone" />
				</span>
			</Group>
			<Text className="stat-value">{value}</Text>
		</>
	);
	return to ? (
		<Paper withBorder className="stat-card" component={Link} to={to}>
			{content}
		</Paper>
	) : (
		<Paper withBorder className="stat-card">
			{content}
		</Paper>
	);
}

export function PanelHeading({
	title,
	action,
}: {
	title: string;
	action?: ReactNode;
}) {
	return (
		<div className="panel-heading">
			<Title order={3}>{title}</Title>
			{action}
		</div>
	);
}

export function ViewAll({
	href,
	children = "View all",
}: {
	href: string;
	children?: ReactNode;
}) {
	return (
		<Button
			component={Link}
			to={href}
			variant="subtle"
			color="gray"
			size="compact-xs"
			rightSection={<ArrowRightIcon size={13} />}
		>
			{children}
		</Button>
	);
}
