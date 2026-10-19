import {
	Group,
	Paper,
	SegmentedControl,
	Select,
	SimpleGrid,
	Stack,
	TextInput,
} from "@mantine/core";
import {
	CheckCircleIcon,
	ClockIcon,
	DownloadSimpleIcon,
	HardDrivesIcon,
	MagnifyingGlassIcon,
} from "@phosphor-icons/react";
import { createFileRoute } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { bytes } from "../admin/api";
import { useAdmin } from "../admin/context";
import {
	CourseStructureTree,
	DownloadList,
	DownloadStructureModal,
	type StructureFilter,
} from "../admin/downloads";
import { PageHeading, PanelHeading, StatCard } from "../admin/shared";
import type { CourseDownloadStatus, DownloadJob } from "../gen/proto/admin_pb";

export const Route = createFileRoute("/admin/downloads")({
	component: Downloads,
});

function Downloads() {
	const { data } = useAdmin();
	const [filter, setFilter] = useState<StructureFilter>("active");
	const [specializationFilter, setSpecializationFilter] =
		useState<string>("all");
	const [query, setQuery] = useState("");
	const [modalTarget, setModalTarget] = useState<{
		title: string;
		courseIds: bigint[];
	} | null>(null);

	const filteredStructureCourses = useMemo(() => {
		if (!data) return [];
		return data.courseDownloads.filter(
			(c) =>
				specializationFilter === "all" ||
				(specializationFilter === "standalone"
					? c.specializationIds.length === 0
					: c.specializationIds.some(
							(id) => String(id) === specializationFilter,
						)),
		);
	}, [data, specializationFilter]);

	const summary = useMemo(() => {
		let totalItems = 0;
		let downloadedItems = 0;
		let remainingItems = 0;
		for (const c of filteredStructureCourses) {
			totalItems += c.totalItems;
			downloadedItems += c.downloadedItems;
			remainingItems += c.remainingItems;
		}
		return { totalItems, downloadedItems, remainingItems };
	}, [filteredStructureCourses]);

	if (!data) return null;

	const activeJobs = data.downloads.filter((j) =>
		["planning", "queued", "running", "paused"].includes(j.state),
	);

	const jobs = data.downloads.filter((job) => {
		const matchesState =
			filter === "active"
				? ["planning", "queued", "running", "paused"].includes(job.state)
				: filter === "downloaded"
					? job.state === "completed"
					: ["failed", "partial_failure", "cancelled"].includes(job.state);
		return (
			matchesState &&
			job.courseIds.some((id) =>
				filteredStructureCourses.some((course) => course.courseId === id),
			) &&
			job.name.toLowerCase().includes(query.trim().toLowerCase())
		);
	});

	const inspectJob = (job: DownloadJob) => {
		setModalTarget({
			title: job.name,
			courseIds: job.courseIds,
		});
	};

	const inspectCourse = (course: CourseDownloadStatus) => {
		setModalTarget({
			title: course.name,
			courseIds: [course.courseId],
		});
	};

	return (
		<>
			<PageHeading title="Downloads" />

			<SimpleGrid cols={{ base: 2, lg: 4 }} spacing="md" className="stats-grid">
				<StatCard
					title="Downloaded"
					value={summary.downloadedItems}
					icon={CheckCircleIcon}
				/>
				<StatCard
					title="Not downloaded"
					value={summary.remainingItems}
					icon={ClockIcon}
				/>
				<StatCard
					title="Active jobs"
					value={activeJobs.length}
					icon={DownloadSimpleIcon}
				/>
				<StatCard
					title="Storage"
					value={bytes(data.storageBytes)}
					icon={HardDrivesIcon}
				/>
			</SimpleGrid>

			<Stack gap="md">
				<Paper withBorder className="panel">
					<div className="catalog-toolbar">
						<TextInput
							aria-label="Search courses, modules, lessons, or items"
							placeholder="Search downloads…"
							leftSection={<MagnifyingGlassIcon size={16} />}
							value={query}
							onChange={(e) => setQuery(e.currentTarget.value)}
							size="xs"
							className="catalog-search"
						/>
						<Group gap="xs" wrap="wrap">
							<Select
								aria-label="Filter by specialisation"
								value={specializationFilter}
								onChange={(v) => setSpecializationFilter(v || "all")}
								data={[
									{ value: "all", label: "All courses" },
									{ value: "standalone", label: "Standalone courses" },
									...data.specializations.map((s) => ({
										value: String(s.id),
										label: s.name,
									})),
								]}
								size="xs"
								w={190}
								allowDeselect={false}
								searchable
							/>
							<SegmentedControl
								size="xs"
								value={filter}
								onChange={(v) => setFilter(v as StructureFilter)}
								data={[
									{ value: "active", label: "Active" },
									{
										value: "downloaded",
										label: `Downloaded (${summary.downloadedItems})`,
									},
									{
										value: "remaining",
										label: `Not downloaded (${summary.remainingItems})`,
									},
								]}
							/>
						</Group>
					</div>
				</Paper>
				<Paper withBorder className="panel">
					<PanelHeading
						title={
							filter === "active"
								? "Download queue"
								: filter === "downloaded"
									? "Completed downloads"
									: "Downloads to retry"
						}
					/>
					<DownloadList jobs={jobs} onInspectJob={inspectJob} />
				</Paper>
				<Paper withBorder className="panel">
					<PanelHeading title="Course files" />
					<CourseStructureTree
						courses={filteredStructureCourses}
						filter={filter}
						query={query}
						onInspectCourse={inspectCourse}
					/>
				</Paper>
			</Stack>

			{modalTarget && (
				<DownloadStructureModal
					title={modalTarget.title}
					courses={data.courseDownloads.filter((course) =>
						modalTarget.courseIds.includes(course.courseId),
					)}
					close={() => setModalTarget(null)}
				/>
			)}
		</>
	);
}
