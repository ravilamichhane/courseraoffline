import { Button, Paper, SimpleGrid, Stack, Text } from "@mantine/core";
import {
	BooksIcon,
	DownloadSimpleIcon,
	HardDrivesIcon,
	PlusIcon,
	SquaresFourIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { bytes } from "../admin/api";
import { useAdmin } from "../admin/context";
import { DownloadList } from "../admin/downloads";
import {
	EmptyState,
	PageHeading,
	PanelHeading,
	StatCard,
	StatusBadge,
	ViewAll,
} from "../admin/shared";

export const Route = createFileRoute("/admin/")({ component: Overview });

function Overview() {
	const { data } = useAdmin();
	if (!data) return null;
	const { courses, specializations, downloads } = data;
	const active = downloads.filter((j) =>
		["running", "queued", "planning", "paused"].includes(j.state),
	);
	return (
		<>
			<PageHeading
				title="Overview"
				action={
					<Button
						component={Link}
						to="/admin/courses"
						search={{ add: true }}
						leftSection={<PlusIcon size={15} />}
						size="xs"
					>
						Import course
					</Button>
				}
			/>
			<SimpleGrid cols={{ base: 2, lg: 4 }} spacing="md" className="stats-grid">
				<StatCard
					title="Courses"
					value={courses.length}
					icon={BooksIcon}
					to="/admin/courses"
				/>
				<StatCard
					title="Specialisations"
					value={specializations.length}
					icon={SquaresFourIcon}
					to="/admin/specializations"
				/>
				<StatCard
					title="Active downloads"
					value={active.length}
					icon={DownloadSimpleIcon}
					to="/admin/downloads"
				/>
				<StatCard
					title="Storage"
					value={bytes(data.storageBytes)}
					icon={HardDrivesIcon}
				/>
			</SimpleGrid>
			<Stack gap="md">
				<Paper withBorder className="panel">
					<PanelHeading
						title="Recent courses"
						action={<ViewAll href="/admin/courses" />}
					/>
					{courses.length ? (
						courses.slice(0, 5).map((c) => (
							<Link
								className="recent-course"
								key={String(c.id)}
								to="/admin/courses"
								search={{ edit: String(c.id) }}
							>
								<span className="course-tile">
									<BooksIcon size={18} weight="duotone" />
								</span>
								<div className="recent-course-text">
									<Text fw={500} size="sm" lineClamp={1}>
										{c.name}
									</Text>
									<Text size="xs" c="dimmed" mt={2}>
										{c.moduleCount} modules <span>·</span>{" "}
										{c.specializationIds.length} specialisations
									</Text>
								</div>
								<StatusBadge
									state={c.archived ? "archived" : c.metadataState}
								/>
							</Link>
						))
					) : (
						<EmptyState
							title="No courses yet"
							action={
								<Button
									component={Link}
									to="/admin/courses"
									search={{ add: true }}
									variant="light"
									size="xs"
									leftSection={<PlusIcon size={14} />}
								>
									Import course
								</Button>
							}
						/>
					)}
				</Paper>
				<Paper withBorder className="panel">
					<PanelHeading
						title="Recent downloads"
						action={<ViewAll href="/admin/downloads" />}
					/>
					<DownloadList jobs={downloads.slice(0, 3)} compact />
				</Paper>
			</Stack>
		</>
	);
}
