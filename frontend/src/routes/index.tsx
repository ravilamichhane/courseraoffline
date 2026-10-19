import {
	ActionIcon,
	Alert,
	Badge,
	Button,
	Group,
	Loader,
	Progress,
	SimpleGrid,
	Stack,
	Text,
	TextInput,
	Title,
} from "@mantine/core";
import {
	ArrowClockwiseIcon,
	ArrowRightIcon,
	BookOpenTextIcon,
	BooksIcon,
	CheckCircleIcon,
	DownloadSimpleIcon,
	GearSixIcon,
	MagnifyingGlassIcon,
	PlayIcon,
	PlusIcon,
	SignOutIcon,
	SquaresFourIcon,
	WarningCircleIcon,
	XIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { bytes } from "../admin/api";
import { useAdmin } from "../admin/context";
import { EmptyState } from "../admin/shared";
import { learningProgress } from "../learner/progress";

export const Route = createFileRoute("/")({ component: LibraryPage });

type ViewFilter = "all" | "specializations" | "courses" | "downloaded";

function LibraryPage() {
	const { data, loading, loadError, refresh, logout } = useAdmin();
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState<ViewFilter>("all");
	const [refreshing, setRefreshing] = useState(false);
	const normalizedQuery = query.trim().toLowerCase();

	const courses = useMemo(() => {
		const downloads = new Map(
			data?.courseDownloads.map((item) => [item.courseId, item]),
		);
		return (data?.courses ?? [])
			.filter((course) => !course.archived)
			.map((course) => {
				const download = downloads.get(course.id);
				return { course, download, progress: learningProgress(download) };
			});
	}, [data]);
	const courseMap = useMemo(
		() => new Map(courses.map((entry) => [entry.course.id, entry])),
		[courses],
	);
	const specializations = useMemo(
		() =>
			(data?.specializations ?? []).map((spec) => {
				const entries = spec.courseIds.flatMap((id) => {
					const entry = courseMap.get(id);
					return entry ? [entry] : [];
				});
				const total = entries.reduce(
					(sum, entry) => sum + entry.progress.total,
					0,
				);
				const completed = entries.reduce(
					(sum, entry) => sum + entry.progress.completed,
					0,
				);
				const nextCourse =
					entries.find(
						(entry) =>
							entry.progress.completed < entry.progress.total &&
							(entry.download?.downloadedItems ?? 0) > 0,
					) ??
					entries.find(
						(entry) => entry.progress.completed < entry.progress.total,
					) ??
					entries[0];
				return {
					spec,
					entries,
					total,
					completed,
					percent: total ? Math.round((completed / total) * 100) : 0,
					nextCourse,
				};
			}),
		[data, courseMap],
	);
	const filteredSpecializations = specializations.filter(
		({ spec, entries }) =>
			!normalizedQuery ||
			`${spec.name} ${spec.slug} ${entries.map(({ course }) => course.name).join(" ")}`
				.toLowerCase()
				.includes(normalizedQuery),
	);
	const filteredCourses = courses.filter(
		({ course, download }) =>
			(filter !== "downloaded" || (download?.downloadedItems ?? 0) > 0) &&
			(!normalizedQuery ||
				`${course.name} ${course.slug}`
					.toLowerCase()
					.includes(normalizedQuery)),
	);
	const offlineCount = courses.filter(
		({ download }) =>
			download &&
			download.totalItems > 0 &&
			download.downloadedItems === download.totalItems,
	).length;
	const suggestedCourse =
		courses.find(
			({ progress, download }) =>
				progress.completed > 0 &&
				progress.completed < progress.total &&
				(download?.downloadedItems ?? 0) > 0,
		) ??
		specializations.find(
			({ nextCourse }) => (nextCourse?.download?.downloadedItems ?? 0) > 0,
		)?.nextCourse ??
		courses.find(({ download }) => (download?.downloadedItems ?? 0) > 0);
	const showSpecializations = filter === "all" || filter === "specializations";
	const showCourses = filter !== "specializations";
	const resultCount =
		(showSpecializations ? filteredSpecializations.length : 0) +
		(showCourses ? filteredCourses.length : 0);
	const resetFilters = () => {
		setQuery("");
		setFilter("all");
	};

	return (
		<div className="library-shell">
			<header className="library-header">
				<div className="library-header-inner">
					<Link to="/" className="library-brand">
						<span className="library-brand-mark">
							<BookOpenTextIcon size={22} weight="duotone" />
						</span>
						<span>
							courseviewer
							<span className="library-brand-caption">
								Your personal learning space
							</span>
						</span>
					</Link>
					<Group gap="xs" wrap="nowrap">
						<Button
							component={Link}
							to="/admin"
							variant="subtle"
							color="gray"
							size="sm"
							leftSection={<GearSixIcon size={17} />}
						>
							Manage library
						</Button>
						<ActionIcon
							variant="subtle"
							color="gray"
							size="lg"
							aria-label="Sign out"
							title="Sign out"
							onClick={logout}
						>
							<SignOutIcon size={19} />
						</ActionIcon>
					</Group>
				</div>
			</header>

			<main className="library-main">
				<div className="library-intro">
					<div>
						<p className="learner-eyebrow">A SPACE TO KEEP GROWING</p>
						<Title order={1}>
							Your learning library<span className="accent-dot">.</span>
						</Title>
						<Text className="library-intro-copy">
							Explore a new idea, build a skill, or pick up where you left off.
						</Text>
					</div>
					{data && (
						<section className="library-stats" aria-label="Library overview">
							<div>
								<strong>{data.specializations.length}</strong>
								<span>Specialisations</span>
							</div>
							<div>
								<strong>{courses.length}</strong>
								<span>Courses</span>
							</div>
							<div>
								<strong>{offlineCount}</strong>
								<span>Available offline</span>
							</div>
						</section>
					)}
				</div>

				{loadError && (
					<Alert
						color="red"
						title="Could not refresh your library"
						icon={<WarningCircleIcon size={18} />}
						mb="lg"
					>
						<Group justify="space-between">
							<Text size="sm">{loadError}</Text>
							<Button
								size="xs"
								variant="light"
								color="red"
								onClick={() => void refresh()}
							>
								Try again
							</Button>
						</Group>
					</Alert>
				)}

				{loading && !data ? (
					<Stack align="center" py={90} gap="sm" role="status">
						<Loader size="sm" />
						<Text c="dimmed" size="sm">
							Getting your library ready…
						</Text>
					</Stack>
				) : data ? (
					<>
						{suggestedCourse && !normalizedQuery && filter === "all" && (
							<Link
								to="/courses/$courseSlug"
								params={{ courseSlug: suggestedCourse.course.slug }}
								className="library-continue"
							>
								<div className="library-continue-icon" aria-hidden="true">
									<PlayIcon size={27} weight="fill" />
								</div>
								<div className="library-continue-copy">
									<p className="learner-eyebrow">
										{suggestedCourse.progress.completed > 0
											? "CONTINUE LEARNING"
											: "READY WHEN YOU ARE"}
									</p>
									<Title order={2}>{suggestedCourse.course.name.trim()}</Title>
									<Text size="sm" c="dimmed">
										{suggestedCourse.progress.completed > 0
											? `${suggestedCourse.progress.completed} of ${suggestedCourse.progress.total} items completed`
											: "Your next lesson is just a click away."}
									</Text>
								</div>
								<span className="library-continue-action">
									{suggestedCourse.progress.completed > 0
										? "Continue course"
										: "Start learning"}
									<ArrowRightIcon size={18} />
								</span>
							</Link>
						)}

						<div className="library-toolbar">
							<fieldset className="library-filters" aria-label="Filter library">
								{(
									[
										[
											"all",
											"All learning",
											courses.length + specializations.length,
										],
										[
											"specializations",
											"Specialisations",
											specializations.length,
										],
										["courses", "Courses", courses.length],
										[
											"downloaded",
											"Downloaded",
											courses.filter(
												({ download }) => (download?.downloadedItems ?? 0) > 0,
											).length,
										],
									] as const
								).map(([value, label, count]) => (
									<button
										type="button"
										key={value}
										aria-pressed={filter === value}
										className={`library-filter ${filter === value ? "active" : ""}`}
										onClick={() => setFilter(value)}
									>
										{label}
										<span>{count}</span>
									</button>
								))}
							</fieldset>
							<Group gap="xs" wrap="nowrap" className="library-search-group">
								<TextInput
									aria-label="Search library"
									placeholder="Search your library…"
									leftSection={<MagnifyingGlassIcon size={17} />}
									value={query}
									onChange={(e) => setQuery(e.currentTarget.value)}
									className="library-search"
									rightSection={
										query ? (
											<ActionIcon
												variant="subtle"
												color="gray"
												size="sm"
												aria-label="Clear search"
												onClick={() => setQuery("")}
											>
												<XIcon size={14} />
											</ActionIcon>
										) : undefined
									}
								/>
								<ActionIcon
									variant="default"
									size="lg"
									aria-label="Refresh library"
									title="Refresh library"
									loading={refreshing}
									onClick={async () => {
										setRefreshing(true);
										try {
											await refresh();
										} finally {
											setRefreshing(false);
										}
									}}
								>
									<ArrowClockwiseIcon size={17} />
								</ActionIcon>
							</Group>
						</div>
						{normalizedQuery && (
							<Text size="sm" c="dimmed" mb="lg" role="status">
								{resultCount} result{resultCount === 1 ? "" : "s"} for “
								{query.trim()}”
							</Text>
						)}

						{resultCount === 0 ? (
							<div className="library-empty">
								<EmptyState
									icon={BooksIcon}
									title={
										courses.length || specializations.length
											? "Nothing here just yet"
											: "Your next chapter starts here"
									}
									action={
										courses.length || specializations.length ? (
											<Stack align="center" gap="md">
												<Text size="sm" c="dimmed">
													Try another search or explore the rest of your
													library.
												</Text>
												<Button variant="light" onClick={resetFilters}>
													Clear filters
												</Button>
											</Stack>
										) : (
											<Stack align="center" gap="md">
												<Text size="sm" c="dimmed">
													Import your first course to start building your
													personal library.
												</Text>
												<Button
													renderRoot={(props) => (
														<Link
															{...props}
															to="/admin/courses"
															search={{ add: true }}
														/>
													)}
													leftSection={<PlusIcon size={17} />}
												>
													Import a course
												</Button>
											</Stack>
										)
									}
								/>
							</div>
						) : (
							<Stack gap={36}>
								{showSpecializations && filteredSpecializations.length > 0 && (
									<section aria-labelledby="specializations-heading">
										<div className="library-section-heading">
											<div>
												<Title order={2} id="specializations-heading">
													Specialisations{" "}
													<span>{filteredSpecializations.length}</span>
												</Title>
												<Text size="sm" c="dimmed">
													A focused path. A bigger picture.
												</Text>
											</div>
											<SquaresFourIcon
												size={21}
												className="library-section-icon"
											/>
										</div>
										<SimpleGrid cols={{ base: 1, sm: 2, xl: 3 }} spacing="lg">
											{filteredSpecializations.map(
												(
													{
														spec,
														entries,
														total,
														completed,
														percent,
														nextCourse,
													},
													index,
												) => {
													const content = (
														<>
															<div className="library-spec-top">
																<span
																	className="library-spec-art"
																	aria-hidden="true"
																>
																	<SquaresFourIcon size={30} weight="duotone" />
																</span>
																<Badge variant="light" color="gray" size="sm">
																	{entries.length} course
																	{entries.length === 1 ? "" : "s"}
																</Badge>
															</div>
															<div className="library-spec-body">
																<p className="learner-eyebrow">
																	SPECIALISATION
																</p>
																<Title order={3}>{spec.name.trim()}</Title>
																<Text size="sm" c="dimmed" lineClamp={2}>
																	{spec.tagline.trim() ||
																		"Build your understanding, one course at a time."}
																</Text>
															</div>
															<div className="library-card-progress">
																<div>
																	<span>
																		{completed > 0
																			? `${completed} of ${total} completed`
																			: "Ready to explore"}
																	</span>
																	<strong>{percent}%</strong>
																</div>
																<Progress
																	value={percent}
																	size={4}
																	color={percent === 100 ? "teal" : "myColor"}
																	aria-label={`${spec.name.trim()} learning progress`}
																/>
															</div>
															<div className="library-spec-footer">
																<span>
																	{percent === 100
																		? "Revisit specialisation"
																		: completed > 0
																			? "Continue specialisation"
																			: "Explore specialisation"}
																</span>
																<ArrowRightIcon size={18} />
															</div>
														</>
													);
													return nextCourse ? (
														<Link
															key={String(spec.id)}
															to="/courses/$courseSlug"
															params={{ courseSlug: nextCourse.course.slug }}
															search={{ spec: spec.slug }}
															className="library-spec-card"
															data-tone={index % 3}
														>
															{content}
														</Link>
													) : (
														<div
															key={String(spec.id)}
															className="library-spec-card is-unavailable"
															data-tone={index % 3}
														>
															{content}
															<Text size="xs" c="dimmed" p="md">
																Import courses to begin this specialisation.
															</Text>
														</div>
													);
												},
											)}
										</SimpleGrid>
									</section>
								)}

								{showCourses && filteredCourses.length > 0 && (
									<section aria-labelledby="courses-heading">
										<div className="library-section-heading">
											<div>
												<Title order={2} id="courses-heading">
													{filter === "downloaded"
														? "Downloaded courses"
														: "Courses"}{" "}
													<span>{filteredCourses.length}</span>
												</Title>
												<Text size="sm" c="dimmed">
													{filter === "downloaded"
														? "Your downloaded content, ready to learn."
														: "Find something that sparks your curiosity."}
												</Text>
											</div>
											<BooksIcon size={21} className="library-section-icon" />
										</div>
										<SimpleGrid cols={{ base: 1, sm: 2, xl: 3 }} spacing="lg">
											{filteredCourses.map(({ course, download, progress }) => {
												const specName = data.specializations
													.find((spec) => spec.courseIds.includes(course.id))
													?.name.trim();
												const fullyDownloaded =
													download &&
													download.totalItems > 0 &&
													download.downloadedItems === download.totalItems;
												return (
													<Link
														key={String(course.id)}
														to="/courses/$courseSlug"
														params={{ courseSlug: course.slug }}
														className="library-course-card"
													>
														<div className="library-course-body">
															<div className="library-course-top">
																<span className="library-course-image">
																	{course.imageUrl ? (
																		<img
																			src={course.imageUrl}
																			alt=""
																			loading="lazy"
																			onError={(e) => {
																				e.currentTarget.style.display = "none";
																			}}
																		/>
																	) : null}
																	<BookOpenTextIcon
																		size={25}
																		weight="duotone"
																	/>
																</span>
																<span
																	className={`library-availability ${fullyDownloaded ? "is-ready" : ""}`}
																>
																	{fullyDownloaded ? (
																		<CheckCircleIcon size={14} weight="fill" />
																	) : (
																		<DownloadSimpleIcon size={14} />
																	)}
																	{fullyDownloaded
																		? "Available offline"
																		: download?.state === "downloading" ||
																				download?.state === "queued"
																			? "Downloading"
																			: (download?.downloadedItems ?? 0) > 0
																				? "Partly downloaded"
																				: "Not downloaded"}
																</span>
															</div>
															<div>
																<Text
																	className="library-course-collection"
																	lineClamp={1}
																>
																	{specName || "INDIVIDUAL COURSE"}
																</Text>
																<Title order={3} lineClamp={2}>
																	{course.name.trim()}
																</Title>
															</div>
															<Text size="sm" c="dimmed">
																{download?.moduleCount ?? course.moduleCount}{" "}
																modules
																{download
																	? ` · ${download.lessonCount} lessons`
																	: ""}
																{download && download.downloadedBytes > 0n
																	? ` · ${bytes(download.downloadedBytes)}`
																	: ""}
															</Text>
															<div className="library-card-progress">
																<div>
																	<span>
																		{progress.completed
																			? `${progress.completed} of ${progress.total} completed`
																			: "Not started"}
																	</span>
																	<strong>{progress.percent}%</strong>
																</div>
																<Progress
																	value={progress.percent}
																	size={4}
																	color={
																		progress.percent === 100
																			? "teal"
																			: "myColor"
																	}
																	aria-label={`${course.name.trim()} learning progress`}
																/>
															</div>
														</div>
														<div className="library-course-footer">
															<span>
																{progress.percent === 100
																	? "Revisit course"
																	: progress.completed > 0
																		? "Continue learning"
																		: "Open course"}
															</span>
															<ArrowRightIcon size={17} />
														</div>
													</Link>
												);
											})}
										</SimpleGrid>
									</section>
								)}
							</Stack>
						)}
						<footer className="library-footer">
							<BookOpenTextIcon size={16} />
							<Text size="xs">Your library. Your pace.</Text>
						</footer>
					</>
				) : null}
			</main>
		</div>
	);
}
