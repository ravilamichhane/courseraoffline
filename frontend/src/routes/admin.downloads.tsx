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
	const [jobFilter, setJobFilter] = useState("all");
	const [structureFilter, setStructureFilter] =
		useState<StructureFilter>("all");
	const [specializationFilter, setSpecializationFilter] =
		useState<string>("all");
	const [query, setQuery] = useState("");
	const [modalTarget, setModalTarget] = useState<{
		title: string;
		courses: CourseDownloadStatus[];
	} | null>(null);

	const filteredStructureCourses = useMemo(() => {
		if (!data) return [];
		return data.courseDownloads.filter(
			(c) =>
				specializationFilter === "all" ||
				c.specializationIds.some((id) => String(id) === specializationFilter),
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

	const jobs = data.downloads.filter(
		(j) =>
			jobFilter === "all" ||
			(jobFilter === "active"
				? ["planning", "queued", "running", "paused"].includes(j.state)
				: jobFilter === "attention"
					? ["failed", "partial_failure"].includes(j.state)
					: j.state === jobFilter),
	);

	const inspectJob = (job: DownloadJob) => {
		const courses = data.courseDownloads.filter((c) =>
			job.courseIds.includes(c.courseId),
		);
		setModalTarget({
			title: job.name,
			courses,
		});
	};

	const inspectCourse = (course: CourseDownloadStatus) => {
		setModalTarget({
			title: course.name,
			courses: [course],
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
					title="Remaining"
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
					<PanelHeading title="Course structure" />
					<div className="catalog-toolbar">
						<TextInput
							aria-label="Search courses, modules, lessons, or items"
							placeholder="Search structure…"
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
									{ value: "all", label: "All specialisations" },
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
								value={structureFilter}
								onChange={(v) => setStructureFilter(v as StructureFilter)}
								data={[
									{ value: "all", label: `All (${summary.totalItems})` },
									{
										value: "downloaded",
										label: `Downloaded (${summary.downloadedItems})`,
									},
									{
										value: "remaining",
										label: `Remaining (${summary.remainingItems})`,
									},
									{ value: "attention", label: "Active / Failed" },
								]}
							/>
						</Group>
					</div>
					<CourseStructureTree
						courses={filteredStructureCourses}
						filter={structureFilter}
						query={query}
						onInspectCourse={inspectCourse}
					/>
				</Paper>

				<Paper withBorder className="panel">
					<PanelHeading
						title="Download queue"
						action={
							<SegmentedControl
								size="xs"
								value={jobFilter}
								onChange={setJobFilter}
								data={[
									{ value: "all", label: `All (${data.downloads.length})` },
									{ value: "active", label: "Active" },
									{ value: "completed", label: "Completed" },
									{ value: "attention", label: "Failed" },
									{ value: "cancelled", label: "Cancelled" },
								]}
							/>
						}
					/>
					<DownloadList jobs={jobs} onInspectJob={inspectJob} />
				</Paper>
			</Stack>

			{modalTarget && (
				<DownloadStructureModal
					title={modalTarget.title}
					courses={modalTarget.courses}
					close={() => setModalTarget(null)}
				/>
			)}
		</>
	);
}
