import {
	ActionIcon,
	Badge,
	Button,
	Group,
	Loader,
	Paper,
	Progress,
	Stack,
	Text,
	TextInput,
	Title,
} from "@mantine/core";
import {
	ArrowLeftIcon,
	ArrowSquareOutIcon,
	BookOpenTextIcon,
	CaretDownIcon,
	CaretLeftIcon,
	CaretRightIcon,
	CheckCircleIcon,
	DownloadSimpleIcon,
	FileTextIcon,
	GearSixIcon,
	MagnifyingGlassIcon,
	PlayCircleIcon,
	QuestionIcon,
	SidebarSimpleIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { adminClient, bytes, mediaUrl } from "../admin/api";
import { useAdmin } from "../admin/context";
import { EmptyState } from "../admin/shared";
import type {
	ItemDownloadStatus,
	LessonDownloadStatus,
	ModuleDownloadStatus,
} from "../gen/proto/admin_pb";

export const Route = createFileRoute("/courses/$courseSlug")({
	validateSearch: (
		search: Record<string, unknown>,
	): { item?: string; spec?: string } => ({
		item:
			typeof search.item === "string" && search.item.trim().length > 0
				? search.item.trim()
				: undefined,
		spec:
			typeof search.spec === "string" && search.spec.trim().length > 0
				? search.spec.trim()
				: undefined,
	}),
	component: CourseViewerPage,
});

type FlattenedItem = {
	module: ModuleDownloadStatus;
	lesson: LessonDownloadStatus;
	item: ItemDownloadStatus;
};

function itemTypeLabel(contentType: string) {
	if (contentType === "lecture") return "Video";
	if (contentType === "supplement" || contentType === "reading")
		return "Reading";
	if (contentType === "ungradedAssignment" || contentType === "staffGraded")
		return "Quiz";
	return contentType;
}

function CourseViewerPage() {
	const { courseSlug } = Route.useParams();
	const search = Route.useSearch();
	const navigate = Route.useNavigate();
	const { data, loading, run } = useAdmin();

	const [sidebarOpen, setSidebarOpen] = useState(true);
	const [outlineQuery, setOutlineQuery] = useState("");
	const [collapsedModules, setCollapsedModules] = useState<
		Record<string, boolean>
	>({});
	const [readingHtml, setReadingHtml] = useState<string | null>(null);
	const [readingLoading, setReadingLoading] = useState(false);
	const [readingError, setReadingError] = useState<string | null>(null);
	const [startingDownload, setStartingDownload] = useState(false);
	const [togglingCompletedId, setTogglingCompletedId] = useState<bigint | null>(
		null,
	);

	const readingRef = useRef<HTMLDivElement | null>(null);

	const course = useMemo(
		() => data?.courses.find((c) => c.slug === courseSlug),
		[data, courseSlug],
	);

	const courseDownload = useMemo(
		() =>
			data?.courseDownloads.find(
				(cd) => cd.slug === courseSlug || (course && cd.courseId === course.id),
			),
		[data, courseSlug, course],
	);

	const courseSpecializations = useMemo(() => {
		if (!data || !course) return [];
		return data.specializations.filter((s) => s.courseIds.includes(course.id));
	}, [data, course]);

	const specialization = useMemo(() => {
		if (!courseSpecializations.length) return undefined;
		if (search.spec) {
			const matched = courseSpecializations.find(
				(s) => s.slug === search.spec || String(s.id) === search.spec,
			);
			if (matched) return matched;
		}
		return courseSpecializations[0];
	}, [courseSpecializations, search.spec]);

	const siblingCourses = useMemo(() => {
		if (!data || !specialization) return [];
		return specialization.courseIds
			.map((id) => data.courses.find((c) => c.id === id))
			.filter((c): c is NonNullable<typeof c> => Boolean(c));
	}, [data, specialization]);

	const flatItems = useMemo<FlattenedItem[]>(() => {
		if (!courseDownload) return [];
		const list: FlattenedItem[] = [];
		for (const mod of courseDownload.modules) {
			for (const les of mod.lessons) {
				for (const item of les.items) {
					list.push({ module: mod, lesson: les, item });
				}
			}
		}
		return list;
	}, [courseDownload]);

	const activeEntry = useMemo<FlattenedItem | undefined>(() => {
		if (!flatItems.length) return undefined;
		if (search.item) {
			const match = flatItems.find(
				(entry) =>
					String(entry.item.id) === search.item ||
					entry.item.slug === search.item,
			);
			if (match) return match;
		}
		return (
			flatItems.find(
				(entry) => entry.item.state === "downloaded" && !entry.item.completed,
			) ||
			flatItems.find((entry) => entry.item.state === "downloaded") ||
			flatItems[0]
		);
	}, [flatItems, search.item]);

	const activeIndex = useMemo(() => {
		if (!activeEntry) return -1;
		return flatItems.findIndex(
			(entry) => entry.item.id === activeEntry.item.id,
		);
	}, [flatItems, activeEntry]);

	const prevEntry = activeIndex > 0 ? flatItems[activeIndex - 1] : undefined;
	const nextEntry =
		activeIndex >= 0 && activeIndex < flatItems.length - 1
			? flatItems[activeIndex + 1]
			: undefined;

	const selectItem = (itemId: bigint) => {
		void navigate({
			search: { item: String(itemId), spec: search.spec },
		});
	};

	const toggleItemCompleted = async (itemId: bigint, completed: boolean) => {
		setTogglingCompletedId(itemId);
		await run(() => adminClient.setItemCompleted({ itemId, completed }));
		setTogglingCompletedId(null);
	};

	const activeItem = activeEntry?.item;

	const videoAsset = useMemo(
		() =>
			activeItem?.assets.find(
				(a) => a.kind === "video" && a.state === "downloaded" && a.relativePath,
			),
		[activeItem],
	);

	const subtitleAssets = useMemo(
		() =>
			activeItem?.assets.filter(
				(a) =>
					a.kind === "subtitle" && a.state === "downloaded" && a.relativePath,
			) || [],
		[activeItem],
	);

	const readingAsset = useMemo(
		() =>
			activeItem?.assets.find(
				(a) =>
					a.kind === "reading" && a.state === "downloaded" && a.relativePath,
			),
		[activeItem],
	);

	const attachmentAssets = useMemo(
		() =>
			activeItem?.assets.filter(
				(a) =>
					!["video", "subtitle", "reading"].includes(a.kind) &&
					a.state === "downloaded" &&
					a.relativePath,
			) || [],
		[activeItem],
	);

	// Load reading HTML when readingAsset changes
	useEffect(() => {
		if (!readingAsset?.relativePath) {
			setReadingHtml(null);
			setReadingError(null);
			setReadingLoading(false);
			return;
		}
		let cancelled = false;
		setReadingLoading(true);
		setReadingError(null);
		fetch(mediaUrl(readingAsset.relativePath))
			.then(async (res) => {
				if (!res.ok) throw new Error(`Failed to load reading (${res.status})`);
				return res.text();
			})
			.then((html) => {
				if (!cancelled) {
					setReadingHtml(html);
					setReadingLoading(false);
				}
			})
			.catch((err) => {
				if (!cancelled) {
					setReadingError(
						err instanceof Error ? err.message : "Could not load reading.",
					);
					setReadingLoading(false);
				}
			});
		return () => {
			cancelled = true;
		};
	}, [readingAsset?.relativePath]);

	// Enhance links and handle broken images inside rendered HTML readings
	useEffect(() => {
		const container = readingRef.current;
		if (!container || !readingHtml) return;
		const cmlNodes = container.querySelectorAll<HTMLElement>(
			".cmlToHtml-content-container",
		);
		for (const node of cmlNodes) {
			node.style.whiteSpace = "normal";
		}
		const links = container.querySelectorAll("a");
		for (const link of links) {
			link.setAttribute("target", "_blank");
			link.setAttribute("rel", "noopener noreferrer");
		}
		const images = container.querySelectorAll("img");
		const handleImgError = (e: Event) => {
			const img = e.currentTarget as HTMLImageElement;
			img.style.display = "none";
		};
		for (const img of images) {
			img.addEventListener("error", handleImgError);
		}
		return () => {
			for (const img of images) {
				img.removeEventListener("error", handleImgError);
			}
		};
	}, [readingHtml]);

	const normalizedOutlineQuery = outlineQuery.trim().toLowerCase();

	const filteredModules = useMemo(() => {
		if (!courseDownload) return [];
		if (!normalizedOutlineQuery) return courseDownload.modules;
		return courseDownload.modules
			.map((mod) => {
				const lessons = mod.lessons
					.map((les) => {
						const items = les.items.filter((item) =>
							`${mod.name} ${les.name} ${item.name}`
								.toLowerCase()
								.includes(normalizedOutlineQuery),
						);
						return { ...les, items };
					})
					.filter((les) => les.items.length > 0);
				return { ...mod, lessons };
			})
			.filter((mod) => mod.lessons.length > 0);
	}, [courseDownload, normalizedOutlineQuery]);

	if (loading && !data) {
		return (
			<Stack align="center" justify="center" h="100vh" gap="xs">
				<Loader size="sm" />
				<Text c="dimmed" size="xs">
					Loading course…
				</Text>
			</Stack>
		);
	}

	if (!course || !courseDownload) {
		return (
			<div className="library-shell">
				<header className="library-header">
					<div className="library-header-inner">
						<Link to="/" className="library-brand">
							<BookOpenTextIcon size={20} weight="duotone" />
							<span>courseviewer</span>
						</Link>
						<Button
							component={Link}
							to="/"
							variant="default"
							size="xs"
							leftSection={<ArrowLeftIcon size={14} />}
						>
							Back to library
						</Button>
					</div>
				</header>
				<main className="library-main">
					<Paper withBorder className="panel">
						<EmptyState
							title="Course not found"
							action={
								<Button component={Link} to="/" variant="light" size="xs">
									Return to library
								</Button>
							}
						/>
					</Paper>
				</main>
			</div>
		);
	}

	const totalCourseItems = flatItems.length;
	const completedCourseItems = flatItems.filter(
		(entry) => entry.item.completed,
	).length;
	const completionProgress =
		totalCourseItems > 0
			? Math.round((completedCourseItems / totalCourseItems) * 100)
			: 0;

	return (
		<div className="viewer-shell">
			<aside
				className={`viewer-sidebar ${sidebarOpen ? "is-open" : "is-collapsed"}`}
			>
				<div className="viewer-sidebar-top">
					<Button
						component={Link}
						to="/"
						variant="subtle"
						color="gray"
						size="compact-xs"
						leftSection={<ArrowLeftIcon size={14} />}
					>
						Library
					</Button>
					<Button
						component={Link}
						to="/admin"
						variant="subtle"
						color="gray"
						size="compact-xs"
						leftSection={<GearSixIcon size={14} />}
					>
						Admin
					</Button>
				</div>

				<div className="viewer-course-summary">
					{specialization && (
						<div className="viewer-spec-switcher">
							{courseSpecializations.length > 1 ? (
								<div className="viewer-spec-pills" style={{ marginBottom: 6 }}>
									{courseSpecializations.map((specItem) => (
										<Link
											key={String(specItem.id)}
											to="/courses/$courseSlug"
											params={{ courseSlug: course.slug }}
											search={{ item: search.item, spec: specItem.slug }}
											className={`spec-course-pill ${specItem.id === specialization.id ? "active" : ""}`}
											title={specItem.name.trim()}
										>
											{specItem.name.trim()}
										</Link>
									))}
								</div>
							) : (
								<Text size="xs" c="dimmed" lineClamp={1} mb={6}>
									{specialization.name.trim()}
								</Text>
							)}
							{siblingCourses.length > 1 && (
								<div className="viewer-spec-pills">
									{siblingCourses.map((sib, idx) => (
										<Link
											key={String(sib.id)}
											to="/courses/$courseSlug"
											params={{ courseSlug: sib.slug }}
											search={{ spec: specialization.slug }}
											className={`spec-course-pill ${sib.id === course.id ? "active" : ""}`}
											title={sib.name.trim()}
										>
											Course {idx + 1}
										</Link>
									))}
								</div>
							)}
						</div>
					)}

					<Title order={2} className="viewer-course-title">
						{course.name.trim()}
					</Title>

					<Group justify="space-between" gap="xs" mt={10} mb={5}>
						<Text size="xs" c="dimmed">
							{completedCourseItems}/{totalCourseItems} completed
						</Text>
						<Text size="xs" fw={500}>
							{completionProgress}%
						</Text>
					</Group>
					<Progress
						value={completionProgress}
						size={4}
						color={completionProgress === 100 ? "teal" : "myColor"}
					/>

					<TextInput
						placeholder="Filter lessons or items…"
						leftSection={<MagnifyingGlassIcon size={14} />}
						value={outlineQuery}
						onChange={(e) => setOutlineQuery(e.currentTarget.value)}
						size="xs"
						mt={12}
					/>
				</div>

				<div className="viewer-outline">
					{filteredModules.length === 0 ? (
						<div className="viewer-outline-empty">
							<Text size="xs" c="dimmed">
								No items match filter.
							</Text>
						</div>
					) : (
						filteredModules.map((mod) => {
							const key = String(mod.id);
							const containsActive = activeEntry?.module.id === mod.id;
							const isOpen =
								normalizedOutlineQuery.length > 0 ||
								(key in collapsedModules
									? !collapsedModules[key]
									: containsActive || mod.position === 0);
							const modItems = mod.lessons.flatMap((l) => l.items);
							const modCompletedCount = modItems.filter(
								(i) => i.completed,
							).length;

							return (
								<div className="viewer-module" key={key}>
									<button
										type="button"
										className="viewer-module-btn"
										onClick={() =>
											setCollapsedModules((prev) => ({
												...prev,
												[key]: isOpen,
											}))
										}
										aria-expanded={isOpen}
									>
										<span className="structure-caret">
											{isOpen ? (
												<CaretDownIcon size={13} weight="bold" />
											) : (
												<CaretRightIcon size={13} weight="bold" />
											)}
										</span>
										<div style={{ minWidth: 0, flex: 1 }}>
											<Text size="xs" fw={600} lineClamp={1}>
												Module {mod.position + 1}: {mod.name.trim()}
											</Text>
										</div>
										<Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
											{modCompletedCount}/{modItems.length}
										</Text>
									</button>

									{isOpen && (
										<div className="viewer-module-body">
											{mod.lessons.map((les) => (
												<div className="viewer-lesson" key={String(les.id)}>
													<div className="viewer-lesson-label">
														Lesson {les.position + 1}: {les.name.trim()}
													</div>
													<div className="viewer-lesson-items">
														{les.items.map((item) => {
															const isSelected = activeItem?.id === item.id;
															return (
																<button
																	key={String(item.id)}
																	type="button"
																	className={`viewer-item-btn ${isSelected ? "active" : ""} ${item.completed ? "is-completed" : ""}`}
																	onClick={() => selectItem(item.id)}
																>
																	<span className="viewer-item-icon">
																		{item.completed ? (
																			<CheckCircleIcon
																				size={15}
																				weight="fill"
																			/>
																		) : item.contentType === "lecture" ? (
																			<PlayCircleIcon size={15} />
																		) : item.contentType === "supplement" ||
																			item.contentType === "reading" ? (
																			<FileTextIcon size={15} />
																		) : (
																			<QuestionIcon size={15} />
																		)}
																	</span>
																	<span className="viewer-item-name">
																		{item.position + 1}. {item.name.trim()}
																	</span>
																	<span className="viewer-item-tag">
																		{itemTypeLabel(item.contentType)}
																	</span>
																</button>
															);
														})}
													</div>
												</div>
											))}
										</div>
									)}
								</div>
							);
						})
					)}
				</div>
			</aside>

			<div className="viewer-main">
				<header className="viewer-header">
					<Group gap="xs" wrap="nowrap" style={{ minWidth: 0, flex: 1 }}>
						<ActionIcon
							variant="subtle"
							color="gray"
							aria-label="Toggle course sidebar"
							onClick={() => setSidebarOpen((v) => !v)}
						>
							<SidebarSimpleIcon size={18} />
						</ActionIcon>
						{activeEntry && (
							<Group gap={6} wrap="nowrap" style={{ minWidth: 0 }}>
								<Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
									Module {activeEntry.module.position + 1}
								</Text>
								<Text size="xs" c="dimmed">
									/
								</Text>
								<Text size="xs" c="dimmed" lineClamp={1}>
									Lesson {activeEntry.lesson.position + 1}:{" "}
									{activeEntry.lesson.name.trim()}
								</Text>
							</Group>
						)}
					</Group>

					<Group gap={6} wrap="nowrap">
						<Button
							size="compact-xs"
							variant="default"
							disabled={!prevEntry}
							leftSection={<CaretLeftIcon size={13} />}
							onClick={() => prevEntry && selectItem(prevEntry.item.id)}
						>
							Prev
						</Button>
						<Button
							size="compact-xs"
							variant="default"
							disabled={!nextEntry}
							rightSection={<CaretRightIcon size={13} />}
							onClick={() => nextEntry && selectItem(nextEntry.item.id)}
						>
							Next
						</Button>
					</Group>
				</header>

				<div className="viewer-stage">
					{!activeEntry || !activeItem ? (
						<Paper withBorder className="panel">
							<EmptyState title="No course items available" />
						</Paper>
					) : (
						<div className="viewer-content-wrap">
							<div className="viewer-item-header">
								<Title order={1} className="viewer-item-title">
									{activeItem.name.trim()}
								</Title>
								<Group gap="xs" wrap="wrap">
									<Badge size="sm" variant="light" color="gray">
										{itemTypeLabel(activeItem.contentType)}
									</Badge>
									{videoAsset?.height ? (
										<Badge size="sm" variant="outline" color="gray">
											{videoAsset.height}p {videoAsset.format.toUpperCase()}
										</Badge>
									) : null}
									<Button
										size="compact-xs"
										variant={activeItem.completed ? "light" : "default"}
										color={activeItem.completed ? "teal" : "gray"}
										loading={togglingCompletedId === activeItem.id}
										leftSection={
											<CheckCircleIcon
												size={14}
												weight={activeItem.completed ? "fill" : "regular"}
											/>
										}
										onClick={() =>
											void toggleItemCompleted(
												activeItem.id,
												!activeItem.completed,
											)
										}
									>
										{activeItem.completed ? "Completed" : "Mark as completed"}
									</Button>
								</Group>
							</div>

							{videoAsset ? (
								<div className="video-player-frame">
									{/* biome-ignore lint/a11y/useMediaCaption: dynamic subtitle tracks are attached from downloaded assets */}
									<video
										key={videoAsset.relativePath}
										controls
										playsInline
										preload="metadata"
										className="course-video-player"
										src={mediaUrl(videoAsset.relativePath)}
										onEnded={() => {
											if (!activeItem.completed) {
												void toggleItemCompleted(activeItem.id, true);
											}
										}}
									>
										{subtitleAssets.map((sub, idx) => (
											<track
												key={String(sub.id)}
												kind="subtitles"
												src={mediaUrl(sub.relativePath)}
												srcLang={sub.languageCode || "en"}
												label={(sub.languageCode || "EN").toUpperCase()}
												default={idx === 0}
											/>
										))}
									</video>
								</div>
							) : readingAsset ? (
								<Paper withBorder className="reading-card">
									{readingLoading ? (
										<Stack align="center" py={48} gap="xs">
											<Loader size="sm" />
											<Text size="xs" c="dimmed">
												Loading reading…
											</Text>
										</Stack>
									) : readingError ? (
										<EmptyState title={readingError} />
									) : readingHtml ? (
										<div
											ref={readingRef}
											className="reading-prose"
											// biome-ignore lint/security/noDangerouslySetInnerHtml: local downloaded Coursera HTML supplement
											dangerouslySetInnerHTML={{ __html: readingHtml }}
										/>
									) : null}
								</Paper>
							) : !activeItem.downloadable ? (
								<Paper withBorder className="panel">
									<EmptyState
										icon={QuestionIcon}
										title="Interactive Coursera assignment"
										action={
											<Group gap="xs">
												<Button
													component="a"
													href={`https://www.coursera.org/learn/${course.slug}`}
													target="_blank"
													rel="noopener noreferrer"
													variant="default"
													size="xs"
													rightSection={<ArrowSquareOutIcon size={14} />}
												>
													Open on Coursera
												</Button>
												{nextEntry && (
													<Button
														size="xs"
														variant="light"
														rightSection={<CaretRightIcon size={14} />}
														onClick={() => selectItem(nextEntry.item.id)}
													>
														Next item
													</Button>
												)}
											</Group>
										}
									/>
								</Paper>
							) : (
								<Paper withBorder className="panel">
									<EmptyState
										icon={DownloadSimpleIcon}
										title={
											activeItem.state === "downloading" ||
											activeItem.state === "queued"
												? "Item is currently downloading…"
												: "Item not downloaded yet"
										}
										action={
											courseDownload.state !== "downloading" ? (
												<Button
													size="xs"
													variant="light"
													loading={startingDownload}
													leftSection={<DownloadSimpleIcon size={15} />}
													onClick={async () => {
														setStartingDownload(true);
														await run(
															() =>
																adminClient.startDownload({
																	target: {
																		case: "courseId",
																		value: course.id,
																	},
																}),
															"Download queued.",
														);
														setStartingDownload(false);
													}}
												>
													Download course
												</Button>
											) : undefined
										}
									/>
								</Paper>
							)}

							{attachmentAssets.length > 0 && (
								<Paper withBorder className="panel" p="md">
									<Text size="xs" fw={600} mb="xs">
										Attachments
									</Text>
									<Group gap="xs">
										{attachmentAssets.map((att) => (
											<Button
												key={String(att.id)}
												component="a"
												href={mediaUrl(att.relativePath)}
												target="_blank"
												rel="noopener noreferrer"
												size="xs"
												variant="default"
												leftSection={<DownloadSimpleIcon size={14} />}
											>
												{att.variantKey} ({bytes(att.downloadedBytes)})
											</Button>
										))}
									</Group>
								</Paper>
							)}

							<div className="viewer-bottom-nav">
								{prevEntry ? (
									<button
										type="button"
										className="viewer-nav-card"
										onClick={() => selectItem(prevEntry.item.id)}
									>
										<Text size="xs" c="dimmed">
											← Previous
										</Text>
										<Text size="xs" fw={500} lineClamp={1} mt={2}>
											{prevEntry.item.name.trim()}
										</Text>
									</button>
								) : (
									<div />
								)}
								{nextEntry ? (
									<button
										type="button"
										className="viewer-nav-card align-right"
										onClick={() => selectItem(nextEntry.item.id)}
									>
										<Text size="xs" c="dimmed">
											Next →
										</Text>
										<Text size="xs" fw={500} lineClamp={1} mt={2}>
											{nextEntry.item.name.trim()}
										</Text>
									</button>
								) : (
									<div />
								)}
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
