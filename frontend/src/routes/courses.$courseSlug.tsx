import {
	ActionIcon,
	Alert,
	Badge,
	Button,
	Drawer,
	Group,
	Loader,
	Paper,
	Progress,
	Select,
	Stack,
	Text,
	TextInput,
	Title,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import {
	ArrowLeftIcon,
	ArrowRightIcon,
	ArrowSquareOutIcon,
	BookOpenTextIcon,
	CaretDownIcon,
	CaretLeftIcon,
	CaretRightIcon,
	CheckCircleIcon,
	DownloadSimpleIcon,
	FastForwardIcon,
	FileTextIcon,
	GaugeIcon,
	GearSixIcon,
	HouseIcon,
	MagnifyingGlassIcon,
	PlayCircleIcon,
	PlayIcon,
	QuestionIcon,
	RewindIcon,
	SidebarSimpleIcon,
	WarningCircleIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { adminClient, bytes, mediaUrl } from "../admin/api";
import { useAdmin } from "../admin/context";
import { EmptyState } from "../admin/shared";
import type {
	ItemDownloadStatus,
	LessonDownloadStatus,
	ModuleDownloadStatus,
} from "../gen/proto/admin_pb";
import { renderMath } from "../learner/math";
import { learningProgress } from "../learner/progress";

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

const PLAYBACK_RATE_STORAGE_KEY = "courseviewer_playback_rate";
const AUTOPLAY_NEXT_STORAGE_KEY = "courseviewer_autoplay_next";
const SPEED_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2] as const;

function clampPlaybackRate(rate: number): number {
	if (!Number.isFinite(rate)) return 1;
	return Math.round(Math.min(3, Math.max(0.25, rate)) * 100) / 100;
}

function readStoredPlaybackRate(): number {
	if (typeof window === "undefined") return 1;
	try {
		const raw = window.localStorage.getItem(PLAYBACK_RATE_STORAGE_KEY);
		if (!raw) return 1;
		const parsed = Number.parseFloat(raw);
		return Number.isFinite(parsed) && parsed >= 0.25 && parsed <= 3
			? clampPlaybackRate(parsed)
			: 1;
	} catch {
		return 1;
	}
}

function writeStoredPlaybackRate(rate: number): void {
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(PLAYBACK_RATE_STORAGE_KEY, String(rate));
	} catch {
		// Ignore storage errors
	}
}

function readStoredAutoPlayNext(): boolean {
	if (typeof window === "undefined") return false;
	try {
		return window.localStorage.getItem(AUTOPLAY_NEXT_STORAGE_KEY) === "true";
	} catch {
		return false;
	}
}

function writeStoredAutoPlayNext(enabled: boolean): void {
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(AUTOPLAY_NEXT_STORAGE_KEY, String(enabled));
	} catch {
		// Ignore storage errors
	}
}

function formatPlaybackRate(rate: number): string {
	return `${Number(rate.toFixed(2))}×`;
}

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
	const { data, loading, loadError, notice, dismiss, refresh, run } =
		useAdmin();

	const isMobile = useMediaQuery("(max-width: 900px)", false, {
		getInitialValueInEffect: false,
	});
	const [sidebarOpen, setSidebarOpen] = useState(false);
	const [desktopSidebarOpen, setDesktopSidebarOpen] = useState(true);
	const sidebarVisible = isMobile ? sidebarOpen : desktopSidebarOpen;
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
	const [playbackRate, setPlaybackRate] = useState<number>(() =>
		readStoredPlaybackRate(),
	);
	const [autoPlayNext, setAutoPlayNext] = useState<boolean>(() =>
		readStoredAutoPlayNext(),
	);

	const readingRef = useRef<HTMLDivElement | null>(null);
	const stageRef = useRef<HTMLDivElement | null>(null);
	const videoRef = useRef<HTMLVideoElement | null>(null);
	const applyingRateRef = useRef(false);

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
			.filter((c): c is NonNullable<typeof c> => Boolean(c) && !c?.archived);
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

	const recommendedEntry = useMemo<FlattenedItem | undefined>(() => {
		if (!flatItems.length) return undefined;
		return (
			flatItems.find(
				(entry) => entry.item.state === "downloaded" && !entry.item.completed,
			) ||
			flatItems.find((entry) => !entry.item.completed) ||
			flatItems.find((entry) => entry.item.state === "downloaded") ||
			flatItems[0]
		);
	}, [flatItems]);

	const activeEntry = useMemo<FlattenedItem | undefined>(() => {
		if (!flatItems.length || !search.item) return undefined;
		return flatItems.find(
			(entry) =>
				String(entry.item.id) === search.item ||
				entry.item.slug === search.item,
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
		setSidebarOpen(false);
		const entry = flatItems.find((entry) => entry.item.id === itemId);
		if (entry)
			setCollapsedModules((previous) => ({
				...previous,
				[String(entry.module.id)]: false,
			}));
		void navigate({
			search: { item: String(itemId), spec: search.spec },
		});
	};

	const showCourseOverview = () => {
		setSidebarOpen(false);
		void navigate({
			search: { item: undefined, spec: search.spec },
		});
	};

	const changePlaybackRate = useCallback((nextRate: number) => {
		const clamped = clampPlaybackRate(nextRate);
		setPlaybackRate(clamped);
		writeStoredPlaybackRate(clamped);
	}, []);

	const toggleAutoPlayNext = useCallback(() => {
		setAutoPlayNext((previous) => {
			const next = !previous;
			writeStoredAutoPlayNext(next);
			return next;
		});
	}, []);

	const seekBy = useCallback((seconds: number) => {
		const video = videoRef.current;
		if (!video) return;
		const duration = Number.isFinite(video.duration)
			? video.duration
			: Number.POSITIVE_INFINITY;
		video.currentTime = Math.max(
			0,
			Math.min(duration, video.currentTime + seconds),
		);
	}, []);

	const toggleItemCompleted = async (itemId: bigint, completed: boolean) => {
		setTogglingCompletedId(itemId);
		await run(
			() => adminClient.setItemCompleted({ itemId, completed }),
			completed ? "Item marked as completed." : "Item marked as incomplete.",
		);
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

	// Enhance links, handle broken images, and render math inside HTML readings
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

		renderMath(container);

		return () => {
			for (const img of images) {
				img.removeEventListener("error", handleImgError);
			}
		};
	}, [readingHtml]);

	const activeOutlineId = activeEntry ? String(activeEntry.item.id) : undefined;

	useEffect(() => {
		if (activeOutlineId === undefined && !courseSlug) return;
		stageRef.current?.scrollTo({ top: 0 });
	}, [activeOutlineId, courseSlug]);

	useEffect(() => {
		const video = videoRef.current;
		if (!video || !videoAsset?.relativePath) return;
		applyingRateRef.current = true;
		video.defaultPlaybackRate = playbackRate;
		if (Math.abs(video.playbackRate - playbackRate) > 0.001) {
			video.playbackRate = playbackRate;
		}
		const timer = window.setTimeout(() => {
			applyingRateRef.current = false;
		}, 0);
		return () => window.clearTimeout(timer);
	}, [playbackRate, videoAsset?.relativePath]);

	useEffect(() => {
		if (!videoAsset?.relativePath) return;
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.metaKey || event.ctrlKey || event.altKey) return;
			const target = event.target as HTMLElement | null;
			if (
				target &&
				(target.tagName === "INPUT" ||
					target.tagName === "TEXTAREA" ||
					target.tagName === "SELECT" ||
					target.isContentEditable)
			) {
				return;
			}
			if (event.key === "[" || event.key === "<") {
				event.preventDefault();
				changePlaybackRate(Math.max(0.5, playbackRate - 0.25));
			} else if (event.key === "]" || event.key === ">") {
				event.preventDefault();
				changePlaybackRate(Math.min(2.5, playbackRate + 0.25));
			} else if (event.key === "\\") {
				event.preventDefault();
				changePlaybackRate(1);
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [videoAsset?.relativePath, playbackRate, changePlaybackRate]);

	useEffect(() => {
		if (!sidebarVisible || !activeOutlineId) return;
		const selectedItem = document.getElementById(
			`outline-item-${activeOutlineId}`,
		);
		selectedItem?.scrollIntoView({ block: "nearest" });
	}, [sidebarVisible, activeOutlineId]);

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
					{loadError && (
						<Alert
							color="red"
							title="Could not load this course"
							icon={<WarningCircleIcon size={18} />}
							mb="lg"
						>
							<Text size="sm">{loadError}</Text>
							<Button
								variant="light"
								color="red"
								size="xs"
								mt="sm"
								onClick={() => void refresh()}
							>
								Try again
							</Button>
						</Alert>
					)}
					<Paper withBorder className="panel">
						<EmptyState
							title={
								course
									? "This course outline is not ready yet"
									: "Course not found"
							}
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

	const {
		total: totalCourseItems,
		completed: completedCourseItems,
		percent: completionProgress,
	} = learningProgress(courseDownload);

	const sidebar = (
		<aside
			id="course-outline"
			aria-label="Course contents"
			className={`viewer-sidebar ${!isMobile && !desktopSidebarOpen ? "is-collapsed" : ""}`}
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
							<Select
								aria-label="Choose specialisation"
								classNames={{ option: "viewer-course-option" }}
								value={specialization.slug}
								data={courseSpecializations.map((spec) => ({
									value: spec.slug,
									label: spec.name.trim(),
								}))}
								onChange={(value) => {
									if (value)
										void navigate({
											search: { item: search.item, spec: value },
										});
								}}
								size="xs"
								allowDeselect={false}
							/>
						) : (
							<Text size="xs" c="dimmed">
								{specialization.name.trim()}
							</Text>
						)}
						{siblingCourses.length > 1 && (
							<Select
								label="Courses in this specialisation"
								aria-label="Choose course in specialisation"
								value={course.slug}
								data={siblingCourses.map((sibling, index) => ({
									value: sibling.slug,
									label: `${index + 1}. ${sibling.name.trim()}`,
								}))}
								onChange={(value) => {
									if (!value) return;
									setSidebarOpen(false);
									setOutlineQuery("");
									setCollapsedModules({});
									void navigate({
										params: { courseSlug: value },
										search: { spec: specialization.slug },
									});
								}}
								allowDeselect={false}
								size="sm"
								classNames={{ option: "viewer-course-option" }}
								mt="sm"
							/>
						)}
					</div>
				)}

				<button
					type="button"
					className="viewer-course-title-btn"
					onClick={showCourseOverview}
					title="View course overview"
				>
					<Title order={2} className="viewer-course-title">
						{course.name.trim()}
					</Title>
				</button>

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
					aria-label="Course learning progress"
				/>
				<div className="viewer-outline-heading">
					<span>Course contents</span>
					<span>{courseDownload.moduleCount} modules</span>
				</div>

				<TextInput
					aria-label="Search course contents"
					placeholder="Find a lesson…"
					leftSection={<MagnifyingGlassIcon size={14} />}
					value={outlineQuery}
					onChange={(e) => setOutlineQuery(e.currentTarget.value)}
					size="xs"
					mt={12}
				/>
			</div>

			<div className="viewer-outline">
				<div className="viewer-overview-nav">
					<button
						type="button"
						className={`viewer-overview-btn ${!activeEntry ? "active" : ""}`}
						aria-current={!activeEntry ? "page" : undefined}
						onClick={showCourseOverview}
					>
						<HouseIcon size={15} weight={!activeEntry ? "fill" : "regular"} />
						<span>Course overview</span>
					</button>
				</div>
				{filteredModules.length === 0 ? (
					<div className="viewer-outline-empty">
						<Text size="xs" c="dimmed">
							{outlineQuery
								? "No lessons match your search."
								: "The course outline will appear here."}
						</Text>
						{outlineQuery && (
							<Button
								variant="subtle"
								size="xs"
								mt="xs"
								onClick={() => setOutlineQuery("")}
							>
								Clear search
							</Button>
						)}
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
										<span className="viewer-module-label">
											Module {mod.position + 1}
										</span>
										<Text size="sm" fw={600} lineClamp={2}>
											{mod.name.trim()}
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
																id={`outline-item-${item.id}`}
																type="button"
																aria-current={isSelected ? "step" : undefined}
																className={`viewer-item-btn ${isSelected ? "active" : ""} ${item.completed ? "is-completed" : ""}`}
																onClick={() => selectItem(item.id)}
															>
																<span className="viewer-item-icon">
																	{item.completed ? (
																		<CheckCircleIcon size={15} weight="fill" />
																	) : item.contentType === "lecture" ? (
																		<PlayCircleIcon size={15} />
																	) : item.contentType === "supplement" ||
																		item.contentType === "reading" ? (
																		<FileTextIcon size={15} />
																	) : (
																		<QuestionIcon size={15} />
																	)}
																</span>
																<span className="viewer-item-copy">
																	<span className="viewer-item-name">
																		{item.name.trim()}
																	</span>
																	<span className="viewer-item-tag">
																		{itemTypeLabel(item.contentType)}
																		{item.completed
																			? " · Completed"
																			: item.downloadable &&
																					item.state !== "downloaded"
																				? " · Not downloaded"
																				: ""}
																	</span>
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
	);

	return (
		<div className="viewer-shell">
			{isMobile ? (
				<Drawer
					opened={sidebarOpen}
					onClose={() => setSidebarOpen(false)}
					title="Course contents"
					size={360}
					padding={0}
					classNames={{
						content: "viewer-drawer-content",
						body: "viewer-drawer-body",
					}}
				>
					{sidebar}
				</Drawer>
			) : (
				sidebar
			)}

			<div className="viewer-main">
				<header className="viewer-header">
					<Group gap="xs" wrap="nowrap" style={{ minWidth: 0, flex: 1 }}>
						<ActionIcon
							component={Link}
							to="/"
							variant="subtle"
							color="gray"
							aria-label="Back to library"
							title="Back to library"
						>
							<ArrowLeftIcon size={18} />
						</ActionIcon>
						<ActionIcon
							variant="subtle"
							color="gray"
							aria-label={
								sidebarVisible ? "Hide course contents" : "Show course contents"
							}
							aria-expanded={sidebarVisible}
							aria-controls="course-outline"
							title="Course contents"
							onClick={() =>
								isMobile
									? setSidebarOpen((value) => !value)
									: setDesktopSidebarOpen((value) => !value)
							}
						>
							<SidebarSimpleIcon size={18} />
						</ActionIcon>
						<div className="viewer-header-course">
							<button
								type="button"
								className="viewer-header-course-btn"
								onClick={showCourseOverview}
								title="Back to course overview"
							>
								<Text size="sm" fw={600} lineClamp={1}>
									{course.name.trim()}
								</Text>
							</button>
							{activeEntry ? (
								<Group gap={6} wrap="nowrap" style={{ minWidth: 0 }}>
									<Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
										Module {activeEntry.module.position + 1}
									</Text>
									<Text size="xs" c="dimmed">
										/
									</Text>
									<Text size="xs" c="dimmed" lineClamp={1}>
										Lesson {activeEntry.lesson.position + 1}
									</Text>
								</Group>
							) : (
								<Text size="xs" c="dimmed" lineClamp={1}>
									Course overview
								</Text>
							)}
						</div>
					</Group>

					<Group gap={6} wrap="nowrap">
						{activeEntry ? (
							<>
								<Button
									size="compact-xs"
									variant="default"
									disabled={!prevEntry}
									leftSection={<CaretLeftIcon size={13} />}
									onClick={() => prevEntry && selectItem(prevEntry.item.id)}
								>
									Previous
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
							</>
						) : recommendedEntry ? (
							<Button
								size="compact-xs"
								variant="light"
								leftSection={<PlayIcon size={12} weight="fill" />}
								onClick={() => selectItem(recommendedEntry.item.id)}
							>
								{completedCourseItems > 0
									? "Continue learning"
									: "Start course"}
							</Button>
						) : null}
					</Group>
				</header>

				<main className="viewer-stage" ref={stageRef}>
					{loadError && (
						<Alert color="red" title="Could not refresh the course" mb="md">
							<Text size="sm">{loadError}</Text>
							<Button
								size="xs"
								variant="light"
								color="red"
								mt="xs"
								onClick={() => void refresh()}
							>
								Try again
							</Button>
						</Alert>
					)}
					{notice && (
						<Alert
							color={notice.error ? "red" : "teal"}
							withCloseButton
							onClose={dismiss}
							mb="md"
							role="status"
						>
							<Text size="sm">{notice.message}</Text>
						</Alert>
					)}
					{!flatItems.length ? (
						<Paper withBorder className="panel">
							<EmptyState title="No course items available" />
						</Paper>
					) : !activeEntry || !activeItem ? (
						<div className="course-overview">
							<section className="course-overview-hero">
								<div className="course-overview-hero-top">
									<span className="course-overview-thumb" aria-hidden="true">
										{course.imageUrl ? (
											<img
												src={course.imageUrl}
												alt=""
												onError={(e) => {
													e.currentTarget.style.display = "none";
												}}
											/>
										) : null}
										<BookOpenTextIcon size={28} weight="duotone" />
									</span>
									<div className="course-overview-hero-copy">
										<p className="learner-eyebrow">
											{specialization
												? specialization.name.trim().toUpperCase()
												: "COURSE HOME"}
										</p>
										<Title order={1} className="course-overview-title">
											{course.name.trim()}
										</Title>
										{course.description.trim() && (
											<Text className="course-overview-desc" lineClamp={4}>
												{course.description.trim()}
											</Text>
										)}
									</div>
								</div>

								<div className="course-overview-meta">
									<span className="course-overview-meta-pill">
										{courseDownload.moduleCount} modules
									</span>
									<span className="course-overview-meta-pill">
										{courseDownload.lessonCount} lessons
									</span>
									<span className="course-overview-meta-pill">
										{totalCourseItems} items
									</span>
									{course.estimatedWorkload.trim() && (
										<span className="course-overview-meta-pill">
											{course.estimatedWorkload.trim()}
										</span>
									)}
									{courseDownload.downloadedBytes > 0n && (
										<span className="course-overview-meta-pill is-ready">
											{bytes(courseDownload.downloadedBytes)} offline
										</span>
									)}
								</div>

								<div className="course-overview-progress">
									<div className="course-overview-progress-head">
										<span>
											{completedCourseItems > 0
												? `${completedCourseItems} of ${totalCourseItems} items completed`
												: "Ready to start"}
										</span>
										<strong>{completionProgress}%</strong>
									</div>
									<Progress
										value={completionProgress}
										size={6}
										color={completionProgress === 100 ? "teal" : "myColor"}
										aria-label="Course completion progress"
									/>
								</div>

								<div className="course-overview-actions">
									{recommendedEntry && (
										<Button
											size="md"
											leftSection={<PlayIcon size={16} weight="fill" />}
											rightSection={<ArrowRightIcon size={16} />}
											onClick={() => selectItem(recommendedEntry.item.id)}
										>
											{completedCourseItems > 0
												? "Continue learning"
												: "Start course"}
										</Button>
									)}
									{courseDownload.downloadedItems < courseDownload.totalItems &&
										!["downloading", "queued", "planning"].includes(
											courseDownload.state,
										) && (
											<Button
												size="md"
												variant="default"
												loading={startingDownload}
												leftSection={<DownloadSimpleIcon size={16} />}
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
										)}
									{recommendedEntry && (
										<Text size="xs" c="dimmed" className="course-overview-next">
											Up next: {recommendedEntry.item.name.trim()}
										</Text>
									)}
								</div>
							</section>

							<section className="course-overview-syllabus">
								<div className="course-overview-syllabus-head">
									<div>
										<Title order={2}>Course syllabus</Title>
										<Text size="sm" c="dimmed">
											Select any lesson item below to jump straight in.
										</Text>
									</div>
									<Text size="xs" c="dimmed">
										{courseDownload.moduleCount} modules ·{" "}
										{courseDownload.lessonCount} lessons
									</Text>
								</div>

								<div className="syllabus-module-list">
									{courseDownload.modules.map((mod) => {
										const modItems = mod.lessons.flatMap((l) => l.items);
										const modCompleted = modItems.filter(
											(i) => i.completed,
										).length;
										const modPercent = modItems.length
											? Math.round((modCompleted / modItems.length) * 100)
											: 0;
										return (
											<div
												key={String(mod.id)}
												className="syllabus-module-card"
											>
												<div className="syllabus-module-header">
													<div>
														<span className="viewer-module-label">
															Module {mod.position + 1}
														</span>
														<Title order={3}>{mod.name.trim()}</Title>
													</div>
													<div className="syllabus-module-progress">
														<Text size="xs" c="dimmed">
															{modCompleted}/{modItems.length} completed
														</Text>
														<Progress
															value={modPercent}
															size={4}
															w={96}
															color={modPercent === 100 ? "teal" : "myColor"}
															aria-label={`Module ${mod.position + 1} progress`}
														/>
													</div>
												</div>

												<div className="syllabus-lessons">
													{mod.lessons.map((les) => (
														<div
															key={String(les.id)}
															className="syllabus-lesson"
														>
															<div className="syllabus-lesson-title">
																Lesson {les.position + 1}: {les.name.trim()}
															</div>
															<div className="syllabus-item-list">
																{les.items.map((item) => (
																	<button
																		key={String(item.id)}
																		type="button"
																		className={`syllabus-item-row ${item.completed ? "is-completed" : ""}`}
																		onClick={() => selectItem(item.id)}
																	>
																		<span className="syllabus-item-icon">
																			{item.completed ? (
																				<CheckCircleIcon
																					size={16}
																					weight="fill"
																				/>
																			) : item.contentType === "lecture" ? (
																				<PlayCircleIcon size={16} />
																			) : item.contentType === "supplement" ||
																				item.contentType === "reading" ? (
																				<FileTextIcon size={16} />
																			) : (
																				<QuestionIcon size={16} />
																			)}
																		</span>
																		<span className="syllabus-item-name">
																			{item.name.trim()}
																		</span>
																		<span className="syllabus-item-meta">
																			<Badge
																				size="xs"
																				variant="light"
																				color="gray"
																			>
																				{itemTypeLabel(item.contentType)}
																			</Badge>
																			{item.completed ? (
																				<Badge
																					size="xs"
																					variant="light"
																					color="teal"
																				>
																					Completed
																				</Badge>
																			) : item.state === "downloaded" ? (
																				<span className="syllabus-item-state is-ready">
																					Ready
																				</span>
																			) : item.downloadable ? (
																				<span className="syllabus-item-state">
																					Not downloaded
																				</span>
																			) : (
																				<span className="syllabus-item-state">
																					Interactive
																				</span>
																			)}
																		</span>
																	</button>
																))}
															</div>
														</div>
													))}
												</div>
											</div>
										);
									})}
								</div>
							</section>
						</div>
					) : (
						<div className="viewer-content-wrap">
							<div className="viewer-item-header">
								{activeEntry && (
									<p className="learner-eyebrow">
										MODULE {activeEntry.module.position + 1} <span> / </span>{" "}
										{activeEntry.lesson.name.trim()}
									</p>
								)}
								<Title order={1} className="viewer-item-title">
									{activeItem.name.trim()}
								</Title>
								<Group gap="xs" wrap="wrap" className="viewer-item-actions">
									<Badge size="sm" variant="light" color="gray">
										{itemTypeLabel(activeItem.contentType)}
									</Badge>
									{videoAsset?.height ? (
										<Badge size="sm" variant="outline" color="gray">
											{videoAsset.height}p {videoAsset.format.toUpperCase()}
										</Badge>
									) : null}
									<Button
										size="sm"
										className="viewer-complete-button"
										aria-pressed={activeItem.completed}
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
										{activeItem.completed ? "Completed" : "Mark complete"}
									</Button>
								</Group>
							</div>

							{videoAsset ? (
								<div className="video-player-container">
									<div className="video-player-frame">
										{/* biome-ignore lint/a11y/useMediaCaption: dynamic subtitle tracks are attached from downloaded assets */}
										<video
											ref={videoRef}
											key={videoAsset.relativePath}
											controls
											playsInline
											preload="metadata"
											className="course-video-player"
											src={mediaUrl(videoAsset.relativePath)}
											onLoadedMetadata={(e) => {
												applyingRateRef.current = true;
												e.currentTarget.defaultPlaybackRate = playbackRate;
												e.currentTarget.playbackRate = playbackRate;
												window.setTimeout(() => {
													applyingRateRef.current = false;
												}, 0);
											}}
											onPlay={(e) => {
												if (
													Math.abs(
														e.currentTarget.playbackRate - playbackRate,
													) > 0.001
												) {
													applyingRateRef.current = true;
													e.currentTarget.playbackRate = playbackRate;
													window.setTimeout(() => {
														applyingRateRef.current = false;
													}, 0);
												}
											}}
											onRateChange={(e) => {
												if (
													applyingRateRef.current ||
													e.currentTarget.readyState === 0
												) {
													return;
												}
												const current = clampPlaybackRate(
													e.currentTarget.playbackRate,
												);
												if (Math.abs(current - playbackRate) > 0.001) {
													changePlaybackRate(current);
												}
											}}
											onEnded={() => {
												if (!activeItem.completed) {
													void toggleItemCompleted(activeItem.id, true);
												}
												if (autoPlayNext && nextEntry) {
													selectItem(nextEntry.item.id);
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

									<div
										className="video-controls-bar"
										role="toolbar"
										aria-label="Video playback controls"
									>
										<div className="video-controls-group">
											<button
												type="button"
												className="video-tool-btn"
												aria-label="Rewind 10 seconds"
												title="Rewind 10 seconds"
												onClick={() => seekBy(-10)}
											>
												<RewindIcon size={14} weight="bold" />
												<span>10s</span>
											</button>
											<button
												type="button"
												className="video-tool-btn"
												aria-label="Forward 10 seconds"
												title="Forward 10 seconds"
												onClick={() => seekBy(10)}
											>
												<span>10s</span>
												<FastForwardIcon size={14} weight="bold" />
											</button>
										</div>

										<div className="speed-control-group">
											<span className="speed-label">
												<GaugeIcon size={15} />
												<span>Speed</span>
											</span>
											<button
												type="button"
												className="speed-step-btn"
												aria-label="Decrease playback speed"
												title="Decrease speed ( [ )"
												disabled={playbackRate <= 0.25}
												onClick={() => changePlaybackRate(playbackRate - 0.25)}
											>
												−
											</button>
											<fieldset
												className="speed-pills"
												aria-label="Playback speed presets"
											>
												{SPEED_PRESETS.map((rate) => {
													const active = Math.abs(playbackRate - rate) < 0.001;
													return (
														<button
															key={rate}
															type="button"
															aria-pressed={active}
															className={`speed-pill ${active ? "active" : ""}`}
															onClick={() => changePlaybackRate(rate)}
														>
															{formatPlaybackRate(rate)}
														</button>
													);
												})}
											</fieldset>
											<button
												type="button"
												className="speed-step-btn"
												aria-label="Increase playback speed"
												title="Increase speed ( ] )"
												disabled={playbackRate >= 3}
												onClick={() => changePlaybackRate(playbackRate + 0.25)}
											>
												+
											</button>
											{!SPEED_PRESETS.some(
												(rate) => Math.abs(playbackRate - rate) < 0.001,
											) && (
												<Badge
													size="xs"
													variant="light"
													color="myColor"
													className="speed-custom-badge"
												>
													{formatPlaybackRate(playbackRate)}
												</Badge>
											)}
										</div>

										<div className="video-controls-group video-controls-right">
											<button
												type="button"
												aria-pressed={autoPlayNext}
												className={`video-tool-btn ${autoPlayNext ? "active" : ""}`}
												onClick={toggleAutoPlayNext}
												title="Automatically advance to the next item when video ends"
											>
												<span>Auto-play next</span>
											</button>
											<span className="video-shortcut-hint">[ ] speed</span>
										</div>
									</div>
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
											!["downloading", "queued", "planning"].includes(
												courseDownload.state,
											) ? (
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
				</main>
			</div>
		</div>
	);
}
