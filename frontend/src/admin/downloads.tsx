import {
	ActionIcon,
	Badge,
	Button,
	Group,
	Modal,
	Paper,
	Progress,
	SegmentedControl,
	Stack,
	Text,
	TextInput,
} from "@mantine/core";
import {
	ArrowsInSimpleIcon,
	ArrowsOutSimpleIcon,
	BooksIcon,
	CaretDownIcon,
	CaretRightIcon,
	CheckCircleIcon,
	ClockIcon,
	CornersOutIcon,
	DownloadSimpleIcon,
	FileTextIcon,
	FolderSimpleIcon,
	LockSimpleIcon,
	MagnifyingGlassIcon,
	TreeStructureIcon,
	VideoCameraIcon,
	WarningCircleIcon,
	XIcon,
} from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import type {
	AssetDownloadStatus,
	CourseDownloadStatus,
	DownloadJob,
	ItemDownloadStatus,
	LessonDownloadStatus,
	ModuleDownloadStatus,
} from "../gen/proto/admin_pb";
import { adminClient, bytes, date } from "./api";
import { useAdmin } from "./context";
import { EmptyState, StatusBadge } from "./shared";

export type StructureFilter = "all" | "downloaded" | "remaining" | "attention";

function itemMatchesFilter(item: ItemDownloadStatus, filter: StructureFilter) {
	if (filter === "all") return true;
	if (!item.downloadable) return false;
	if (filter === "downloaded") return item.state === "downloaded";
	if (filter === "remaining") return item.state !== "downloaded";
	if (filter === "attention")
		return ["downloading", "queued", "partial", "failed"].includes(item.state);
	return true;
}

function itemMatchesQuery(
	item: ItemDownloadStatus,
	lesson: LessonDownloadStatus,
	module: ModuleDownloadStatus,
	course: CourseDownloadStatus,
	q: string,
) {
	if (!q) return true;
	const target =
		`${course.name} ${course.slug} ${module.name} ${lesson.name} ${item.name} ${item.contentType}`.toLowerCase();
	return target.includes(q);
}

function assetLabel(asset: AssetDownloadStatus) {
	if (asset.kind === "video") {
		return `Video ${asset.height ? `${asset.height}p ` : ""}${asset.format.toUpperCase()}`;
	}
	if (asset.kind === "subtitle") {
		return `Subtitle ${asset.languageCode ? `${asset.languageCode.toUpperCase()} ` : ""}${asset.format.toUpperCase()}`;
	}
	if (asset.kind === "reading") {
		return "Reading HTML";
	}
	return `${asset.kind} ${asset.format.toUpperCase()}`;
}

export function CourseStructureTree({
	courses,
	filter = "all",
	query = "",
	defaultExpandedCourses = false,
	onInspectCourse,
}: {
	courses: CourseDownloadStatus[];
	filter?: StructureFilter;
	query?: string;
	defaultExpandedCourses?: boolean;
	onInspectCourse?: (course: CourseDownloadStatus) => void;
}) {
	const { data, run } = useAdmin();
	const [openCourses, setOpenCourses] = useState<Record<string, boolean>>({});
	const [openModules, setOpenModules] = useState<Record<string, boolean>>({});
	const [openLessons, setOpenLessons] = useState<Record<string, boolean>>({});
	const [expandAll, setExpandAll] = useState<boolean | null>(null);
	const [busyCourseId, setBusyCourseId] = useState<bigint | null>(null);

	const normalizedQuery = query.trim().toLowerCase();

	const filteredCourses = useMemo(() => {
		return courses
			.map((course) => {
				const modules = course.modules
					.map((mod) => {
						const lessons = mod.lessons
							.map((les) => {
								const items = les.items.filter(
									(item) =>
										itemMatchesFilter(item, filter) &&
										itemMatchesQuery(item, les, mod, course, normalizedQuery),
								);
								return { lesson: les, items };
							})
							.filter((l) => l.items.length > 0);
						return { module: mod, lessons };
					})
					.filter((m) => m.lessons.length > 0);

				const courseHeaderMatches =
					(!normalizedQuery ||
						`${course.name} ${course.slug}`
							.toLowerCase()
							.includes(normalizedQuery)) &&
					(filter === "all" ||
						(filter === "downloaded" &&
							course.totalItems > 0 &&
							course.downloadedItems === course.totalItems) ||
						(filter === "remaining" &&
							(course.remainingItems > 0 || course.totalItems === 0)) ||
						(filter === "attention" &&
							["downloading", "partial", "failed"].includes(course.state)));

				return {
					course,
					modules,
					visible: modules.length > 0 || courseHeaderMatches,
				};
			})
			.filter((entry) => entry.visible);
	}, [courses, filter, normalizedQuery]);

	const isCourseOpen = (id: bigint, index: number) => {
		const key = String(id);
		if (key in openCourses) return openCourses[key];
		if (expandAll !== null) return expandAll;
		if (normalizedQuery || filter !== "all") return true;
		return defaultExpandedCourses || courses.length === 1 || index === 0;
	};

	const isModuleOpen = (id: bigint, index: number) => {
		const key = String(id);
		if (key in openModules) return openModules[key];
		if (expandAll !== null) return expandAll;
		if (normalizedQuery || filter !== "all") return true;
		return courses.length === 1 && index === 0;
	};

	const isLessonOpen = (id: bigint, index: number) => {
		const key = String(id);
		if (key in openLessons) return openLessons[key];
		if (expandAll !== null) return expandAll;
		if (normalizedQuery || filter !== "all") return true;
		return courses.length === 1 && index === 0;
	};

	const handleExpandAll = (value: boolean) => {
		setExpandAll(value);
		setOpenCourses({});
		setOpenModules({});
		setOpenLessons({});
	};

	if (!filteredCourses.length) {
		return <EmptyState icon={TreeStructureIcon} title="No matching items" />;
	}

	return (
		<div className="structure-tree">
			<Group justify="flex-end" gap={6} className="structure-tree-controls">
				<Button
					size="compact-xs"
					variant="subtle"
					color="gray"
					leftSection={<ArrowsOutSimpleIcon size={13} />}
					onClick={() => handleExpandAll(true)}
				>
					Expand all
				</Button>
				<Button
					size="compact-xs"
					variant="subtle"
					color="gray"
					leftSection={<ArrowsInSimpleIcon size={13} />}
					onClick={() => handleExpandAll(false)}
				>
					Collapse all
				</Button>
			</Group>
			<div className="structure-course-list">
				{filteredCourses.map(({ course, modules }, courseIdx) => {
					const cOpen = isCourseOpen(course.courseId, courseIdx);
					const progress =
						course.totalItems > 0
							? Math.round((course.downloadedItems / course.totalItems) * 100)
							: 0;
					const specNames = course.specializationIds
						.map((id) => data?.specializations.find((s) => s.id === id)?.name)
						.filter(Boolean);

					return (
						<div className="structure-course" key={String(course.courseId)}>
							<div className="structure-course-header">
								<button
									type="button"
									className="structure-toggle-btn"
									onClick={() =>
										setOpenCourses((prev) => ({
											...prev,
											[String(course.courseId)]: !cOpen,
										}))
									}
									aria-expanded={cOpen}
								>
									<span className="structure-caret">
										{cOpen ? (
											<CaretDownIcon size={14} weight="bold" />
										) : (
											<CaretRightIcon size={14} weight="bold" />
										)}
									</span>
									<span className="course-tile">
										<BooksIcon size={18} weight="duotone" />
									</span>
									<div className="structure-course-title-wrap">
										<Group gap={6} wrap="nowrap">
											<Text fw={500} size="sm" lineClamp={1}>
												{course.name}
											</Text>
											{specNames.map((specName) => (
												<Badge
													key={specName}
													size="xs"
													variant="light"
													color="gray"
													style={{ flexShrink: 0 }}
												>
													{specName}
												</Badge>
											))}
										</Group>
										<Text size="xs" c="dimmed" mt={2}>
											{course.moduleCount} module
											{course.moduleCount === 1 ? "" : "s"} ·{" "}
											{course.lessonCount} lesson
											{course.lessonCount === 1 ? "" : "s"}
											{course.downloadedBytes > 0n &&
												` · ${bytes(course.downloadedBytes)}`}
										</Text>
									</div>
								</button>

								<div className="structure-course-actions">
									<div className="structure-progress-box">
										<Group justify="space-between" gap={6} mb={4}>
											<Text size="xs" c="dimmed">
												{course.downloadedItems}/{course.totalItems}
											</Text>
											<Text size="xs" fw={500}>
												{progress}%
											</Text>
										</Group>
										<Progress
											value={progress}
											size={4}
											color={
												course.state === "failed"
													? "red"
													: course.state === "downloaded"
														? "teal"
														: "myColor"
											}
											aria-label={`Course progress for ${course.name}`}
										/>
									</div>
									<StatusBadge state={course.state} />
									{onInspectCourse && (
										<ActionIcon
											variant="default"
											size="md"
											aria-label={`Inspect ${course.name}`}
											onClick={() => onInspectCourse(course)}
										>
											<CornersOutIcon size={14} />
										</ActionIcon>
									)}
									{(course.remainingItems > 0 ||
										course.outlineState !== "ready") &&
										course.state !== "downloading" && (
											<Button
												size="xs"
												variant="light"
												loading={busyCourseId === course.courseId}
												leftSection={<DownloadSimpleIcon size={14} />}
												onClick={async () => {
													setBusyCourseId(course.courseId);
													await run(
														() =>
															adminClient.startDownload({
																target: {
																	case: "courseId",
																	value: course.courseId,
																},
															}),
														"Download queued.",
													);
													setBusyCourseId(null);
												}}
											>
												Download
											</Button>
										)}
								</div>
							</div>

							{course.lastError && (
								<div className="structure-error-banner">
									<WarningCircleIcon size={14} />
									<span>{course.lastError}</span>
								</div>
							)}

							{cOpen && (
								<div className="structure-modules">
									{modules.length === 0 ? (
										<div className="structure-empty-outline">
											<Text size="xs" c="dimmed">
												{course.outlineState === "ready"
													? "No matching items."
													: "Outline not synced yet."}
											</Text>
										</div>
									) : (
										modules.map(({ module: mod, lessons }, modIdx) => {
											const mOpen = isModuleOpen(mod.id, modIdx);
											const modProgress =
												mod.totalItems > 0
													? Math.round(
															(mod.downloadedItems / mod.totalItems) * 100,
														)
													: 0;
											return (
												<div className="structure-module" key={String(mod.id)}>
													<button
														type="button"
														className="structure-module-header"
														onClick={() =>
															setOpenModules((prev) => ({
																...prev,
																[String(mod.id)]: !mOpen,
															}))
														}
														aria-expanded={mOpen}
													>
														<span className="structure-caret">
															{mOpen ? (
																<CaretDownIcon size={13} weight="bold" />
															) : (
																<CaretRightIcon size={13} weight="bold" />
															)}
														</span>
														<FolderSimpleIcon
															size={16}
															weight="duotone"
															className="structure-module-icon"
														/>
														<div className="structure-module-title">
															<Text size="xs" fw={500} lineClamp={1}>
																Module {mod.position + 1}: {mod.name}
															</Text>
														</div>
														<div className="structure-module-meta">
															<Text size="xs" c="dimmed">
																{mod.downloadedItems}/{mod.totalItems}
															</Text>
															<div className="structure-mini-progress">
																<Progress
																	value={modProgress}
																	size={4}
																	color={
																		mod.state === "failed"
																			? "red"
																			: mod.state === "downloaded"
																				? "teal"
																				: "myColor"
																	}
																/>
															</div>
															<StatusBadge state={mod.state} />
														</div>
													</button>

													{mOpen && (
														<div className="structure-lessons">
															{lessons.map(({ lesson: les, items }, lesIdx) => {
																const lOpen = isLessonOpen(les.id, lesIdx);
																return (
																	<div
																		className="structure-lesson"
																		key={String(les.id)}
																	>
																		<button
																			type="button"
																			className="structure-lesson-header"
																			onClick={() =>
																				setOpenLessons((prev) => ({
																					...prev,
																					[String(les.id)]: !lOpen,
																				}))
																			}
																			aria-expanded={lOpen}
																		>
																			<span className="structure-caret">
																				{lOpen ? (
																					<CaretDownIcon
																						size={12}
																						weight="bold"
																					/>
																				) : (
																					<CaretRightIcon
																						size={12}
																						weight="bold"
																					/>
																				)}
																			</span>
																			<div className="structure-lesson-title">
																				<Text size="xs" fw={500} lineClamp={1}>
																					Lesson {les.position + 1}: {les.name}
																				</Text>
																			</div>
																			<Group gap="xs" wrap="nowrap">
																				<Text size="xs" c="dimmed">
																					{les.downloadedItems}/{les.totalItems}
																				</Text>
																				<StatusBadge state={les.state} />
																			</Group>
																		</button>

																		{lOpen && (
																			<div className="structure-items">
																				{items.map((item) => (
																					<div
																						className={`structure-item ${item.state === "downloaded" ? "is-downloaded" : "is-remaining"}`}
																						key={String(item.id)}
																					>
																						<div className="structure-item-main">
																							<span className="structure-item-icon">
																								{item.isLocked ? (
																									<LockSimpleIcon
																										size={14}
																										color="#dd7777"
																									/>
																								) : item.contentType ===
																									"lecture" ? (
																									<VideoCameraIcon
																										size={14}
																										weight="duotone"
																									/>
																								) : (
																									<FileTextIcon
																										size={14}
																										weight="duotone"
																									/>
																								)}
																							</span>
																							<div className="structure-item-info">
																								<Group gap={6} wrap="wrap">
																									<Text size="xs" fw={500}>
																										{item.position + 1}.{" "}
																										{item.name}
																									</Text>
																									<Badge
																										size="xs"
																										variant="outline"
																										color="gray"
																									>
																										{item.contentType}
																									</Badge>
																									{item.isLocked && (
																										<Badge
																											size="xs"
																											color="red"
																											variant="light"
																										>
																											Locked
																										</Badge>
																									)}
																								</Group>
																								{item.assets.length > 0 && (
																									<div className="structure-assets">
																										{item.assets.map(
																											(asset) => (
																												<span
																													key={String(asset.id)}
																													className={`asset-chip asset-${asset.state}`}
																													title={
																														asset.relativePath ||
																														asset.variantKey
																													}
																												>
																													<span className="asset-dot" />
																													<span>
																														{assetLabel(asset)}
																													</span>
																													{(asset.downloadedBytes >
																														0n ||
																														asset.totalBytes >
																															0n) && (
																														<span className="asset-bytes">
																															{bytes(
																																asset.downloadedBytes >
																																	0n
																																	? asset.downloadedBytes
																																	: asset.totalBytes,
																															)}
																														</span>
																													)}
																												</span>
																											),
																										)}
																									</div>
																								)}
																								{item.lastError && (
																									<Text
																										size="xs"
																										c="red"
																										mt={3}
																									>
																										{item.lastError}
																									</Text>
																								)}
																							</div>
																						</div>
																						<Group gap="xs" wrap="nowrap">
																							{item.downloadedBytes > 0n && (
																								<Text size="xs" c="dimmed">
																									{bytes(item.downloadedBytes)}
																								</Text>
																							)}
																							<StatusBadge state={item.state} />
																						</Group>
																					</div>
																				))}
																			</div>
																		)}
																	</div>
																);
															})}
														</div>
													)}
												</div>
											);
										})
									)}
								</div>
							)}
						</div>
					);
				})}
			</div>
		</div>
	);
}

export function DownloadStructureModal({
	title,
	courses,
	close,
}: {
	title: string;
	subtitle?: string;
	courses: CourseDownloadStatus[];
	close: () => void;
}) {
	const [filter, setFilter] = useState<StructureFilter>("all");
	const [query, setQuery] = useState("");

	const totals = useMemo(() => {
		let modules = 0;
		let lessons = 0;
		let totalItems = 0;
		let downloadedItems = 0;
		let remainingItems = 0;
		let downloadedBytes = 0n;
		for (const c of courses) {
			modules += c.moduleCount;
			lessons += c.lessonCount;
			totalItems += c.totalItems;
			downloadedItems += c.downloadedItems;
			remainingItems += c.remainingItems;
			downloadedBytes += c.downloadedBytes;
		}
		const progress =
			totalItems > 0 ? Math.round((downloadedItems / totalItems) * 100) : 0;
		return {
			modules,
			lessons,
			totalItems,
			downloadedItems,
			remainingItems,
			downloadedBytes,
			progress,
		};
	}, [courses]);

	return (
		<Modal opened onClose={close} title={title} centered size="xl">
			<Stack gap="sm">
				<Paper withBorder className="structure-modal-summary">
					<Group justify="space-between" wrap="wrap" gap="md">
						<Group gap="lg" wrap="wrap">
							<div className="structure-stat-pill">
								<CheckCircleIcon size={16} color="#38d9a9" weight="duotone" />
								<Text size="xs" fw={500}>
									{totals.downloadedItems} downloaded
									{totals.downloadedBytes > 0n
										? ` (${bytes(totals.downloadedBytes)})`
										: ""}
								</Text>
							</div>
							<div className="structure-stat-pill">
								<ClockIcon size={16} color="#f08c00" weight="duotone" />
								<Text size="xs" fw={500}>
									{totals.remainingItems} remaining
								</Text>
							</div>
							<div className="structure-stat-pill">
								<TreeStructureIcon size={16} color="#9398a5" weight="duotone" />
								<Text size="xs" c="dimmed">
									{totals.modules} modules · {totals.lessons} lessons
								</Text>
							</div>
						</Group>
						<div className="structure-modal-progress">
							<Group justify="space-between" mb={4}>
								<Text size="xs" c="dimmed">
									Completion
								</Text>
								<Text size="xs" fw={500}>
									{totals.progress}%
								</Text>
							</Group>
							<Progress
								value={totals.progress}
								color={totals.progress === 100 ? "teal" : "myColor"}
								size={5}
							/>
						</div>
					</Group>
				</Paper>

				<Group justify="space-between" gap="xs" wrap="wrap">
					<TextInput
						placeholder="Filter structure…"
						leftSection={<MagnifyingGlassIcon size={15} />}
						value={query}
						onChange={(e) => setQuery(e.currentTarget.value)}
						size="xs"
						style={{ flex: 1, minWidth: 200 }}
					/>
					<SegmentedControl
						size="xs"
						value={filter}
						onChange={(v) => setFilter(v as StructureFilter)}
						data={[
							{ value: "all", label: `All (${totals.totalItems})` },
							{
								value: "downloaded",
								label: `Downloaded (${totals.downloadedItems})`,
							},
							{
								value: "remaining",
								label: `Remaining (${totals.remainingItems})`,
							},
							{ value: "attention", label: "Active / Failed" },
						]}
					/>
				</Group>

				<div className="structure-modal-scroll">
					<CourseStructureTree
						courses={courses}
						filter={filter}
						query={query}
						defaultExpandedCourses
					/>
				</div>
			</Stack>
		</Modal>
	);
}

export function DownloadList({
	jobs,
	compact = false,
	onInspectJob,
}: {
	jobs: DownloadJob[];
	compact?: boolean;
	onInspectJob?: (job: DownloadJob) => void;
}) {
	const { data, run } = useAdmin();
	const [cancelling, setCancelling] = useState<DownloadJob | null>(null);
	const [inspecting, setInspecting] = useState<DownloadJob | null>(null);
	const [busy, setBusy] = useState(false);

	const openInspect = (job: DownloadJob) => {
		if (onInspectJob) onInspectJob(job);
		else setInspecting(job);
	};

	const inspectedCourses = useMemo(() => {
		if (!inspecting || !data) return [];
		return data.courseDownloads.filter((c) =>
			inspecting.courseIds.includes(c.courseId),
		);
	}, [inspecting, data]);

	return (
		<>
			{jobs.length ? (
				<div className="download-list">
					{jobs.map((job) => {
						const progress =
							job.state === "completed"
								? 100
								: job.taskCount > 0
									? Math.round((job.completedTasks / job.taskCount) * 100)
									: 0;
						return (
							<div className="download-row" key={String(job.id)}>
								<span className="course-tile">
									<DownloadSimpleIcon size={18} weight="duotone" />
								</span>
								<div className="download-details">
									<div className="download-row-header">
										<div style={{ minWidth: 0 }}>
											<button
												type="button"
												className="course-title-button"
												onClick={() => openInspect(job)}
											>
												{job.name}
											</button>
											<Text size="xs" c="dimmed" mt={2}>
												{job.kind === "course" ? "Course" : "Specialisation"} ·{" "}
												{date(job.createdAt)}
												{job.downloadedBytes > 0n &&
													` · ${bytes(job.downloadedBytes)}`}
											</Text>
										</div>
										<Group gap="xs" wrap="nowrap">
											<StatusBadge state={job.state} />
											<Button
												size="compact-xs"
												variant="default"
												leftSection={<TreeStructureIcon size={13} />}
												onClick={() => openInspect(job)}
											>
												Structure
											</Button>
											{!compact &&
												[
													"queued",
													"running",
													"planning",
													"paused",
													"failed",
													"partial_failure",
												].includes(job.state) && (
													<ActionIcon
														size="sm"
														color="gray"
														variant="subtle"
														aria-label={`Cancel ${job.name}`}
														onClick={() => setCancelling(job)}
													>
														<XIcon size={14} />
													</ActionIcon>
												)}
										</Group>
									</div>
									<div className="download-progress-line">
										<Progress
											value={progress}
											color={
												["failed", "partial_failure"].includes(job.state)
													? "red"
													: job.state === "completed"
														? "teal"
														: "myColor"
											}
											size={4}
											style={{ flex: 1 }}
											aria-label={`Download progress for ${job.name}`}
										/>
										<Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
											{job.taskCount
												? `${job.completedTasks}/${job.taskCount}`
												: "0/0"}{" "}
											({progress}%)
										</Text>
									</div>
									{job.lastError && (
										<Text c="red" size="xs" mt={4}>
											{job.lastError}
										</Text>
									)}
								</div>
							</div>
						);
					})}
				</div>
			) : (
				<EmptyState icon={DownloadSimpleIcon} title="No downloads" />
			)}
			{inspecting && (
				<DownloadStructureModal
					title={inspecting.name}
					courses={inspectedCourses}
					close={() => setInspecting(null)}
				/>
			)}
			{cancelling && (
				<Modal
					opened
					centered
					size="sm"
					title="Cancel download?"
					onClose={() => {
						if (!busy) setCancelling(null);
					}}
					closeOnEscape={!busy}
					closeOnClickOutside={!busy}
				>
					<Stack gap="md">
						<Text size="sm">Cancel download for “{cancelling.name}”?</Text>
						<Group justify="flex-end" gap="xs">
							<Button
								variant="default"
								disabled={busy}
								onClick={() => setCancelling(null)}
							>
								Keep
							</Button>
							<Button
								color="red"
								loading={busy}
								onClick={async () => {
									setBusy(true);
									const saved = await run(
										() => adminClient.cancelDownload({ id: cancelling.id }),
										"Download cancelled.",
									);
									setBusy(false);
									if (saved) setCancelling(null);
								}}
							>
								Cancel download
							</Button>
						</Group>
					</Stack>
				</Modal>
			)}
		</>
	);
}
