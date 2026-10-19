import {
	ActionIcon,
	Alert,
	Badge,
	Button,
	Group,
	Loader,
	Paper,
	Progress,
	SegmentedControl,
	SimpleGrid,
	Stack,
	Text,
	TextInput,
	Title,
} from "@mantine/core";
import {
	ArrowClockwiseIcon,
	BookOpenTextIcon,
	BooksIcon,
	CaretRightIcon,
	GearSixIcon,
	MagnifyingGlassIcon,
	PlayIcon,
	PlusIcon,
	SignOutIcon,
	SquaresFourIcon,
	WarningCircleIcon,
} from "@phosphor-icons/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { bytes } from "../admin/api";
import { useAdmin } from "../admin/context";
import { EmptyState, StatusBadge } from "../admin/shared";

export const Route = createFileRoute("/")({
	component: LibraryPage,
});

type ViewFilter = "all" | "specializations" | "courses" | "downloaded";

function LibraryPage() {
	const { data, loading, loadError, refresh, logout } = useAdmin();
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState<ViewFilter>("all");

	const normalizedQuery = query.trim().toLowerCase();

	const activeCourses = useMemo(() => {
		if (!data) return [];
		return data.courses.filter((c) => !c.archived);
	}, [data]);

	const filteredSpecializations = useMemo(() => {
		if (!data) return [];
		return data.specializations.filter((s) => {
			if (!normalizedQuery) return true;
			const specCourseNames = s.courseIds
				.map((id) => data.courses.find((c) => c.id === id)?.name || "")
				.join(" ");
			return `${s.name} ${s.slug} ${specCourseNames}`
				.toLowerCase()
				.includes(normalizedQuery);
		});
	}, [data, normalizedQuery]);

	const filteredCourses = useMemo(() => {
		if (!data) return [];
		return activeCourses.filter((c) => {
			const cd = data.courseDownloads.find((item) => item.courseId === c.id);
			if (filter === "downloaded" && (!cd || cd.downloadedItems === 0)) {
				return false;
			}
			if (!normalizedQuery) return true;
			return `${c.name} ${c.slug}`.toLowerCase().includes(normalizedQuery);
		});
	}, [data, activeCourses, filter, normalizedQuery]);

	return (
		<div className="library-shell">
			<header className="library-header">
				<div className="library-header-inner">
					<Link to="/" className="library-brand">
						<BookOpenTextIcon size={20} weight="duotone" />
						<span>courseviewer</span>
					</Link>

					<Group
						gap="xs"
						wrap="wrap"
						style={{ flex: 1, justifyContent: "flex-end" }}
					>
						<TextInput
							aria-label="Search library"
							placeholder="Search specialisations or courses…"
							leftSection={<MagnifyingGlassIcon size={15} />}
							value={query}
							onChange={(e) => setQuery(e.currentTarget.value)}
							size="xs"
							className="library-search"
						/>
						<SegmentedControl
							size="xs"
							value={filter}
							onChange={(v) => setFilter(v as ViewFilter)}
							data={[
								{ value: "all", label: "All" },
								{ value: "specializations", label: "Specialisations" },
								{ value: "courses", label: "Courses" },
								{ value: "downloaded", label: "Downloaded" },
							]}
						/>
						<ActionIcon
							variant="subtle"
							color="gray"
							aria-label="Refresh library"
							onClick={() => void refresh()}
						>
							<ArrowClockwiseIcon size={16} />
						</ActionIcon>
						<Button
							component={Link}
							to="/admin"
							variant="default"
							size="xs"
							leftSection={<GearSixIcon size={14} />}
						>
							Admin
						</Button>
						<ActionIcon
							variant="subtle"
							color="gray"
							aria-label="Sign out"
							title="Sign out"
							onClick={logout}
						>
							<SignOutIcon size={16} />
						</ActionIcon>
					</Group>
				</div>
			</header>

			<main className="library-main">
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
					<Stack align="center" py={90} gap="xs">
						<Loader size="sm" />
						<Text c="dimmed" size="xs">
							Loading library…
						</Text>
					</Stack>
				) : data ? (
					<Stack gap="xl">
						{(filter === "all" || filter === "specializations") &&
							filteredSpecializations.length > 0 && (
								<section>
									<Group justify="space-between" align="center" mb="sm">
										<Group gap="xs" align="center">
											<Title order={2} className="section-title">
												Specialisations
											</Title>
											<Badge size="xs" variant="light" color="gray">
												{filteredSpecializations.length}
											</Badge>
										</Group>
									</Group>
									<SimpleGrid cols={{ base: 1, lg: 2 }} spacing="md">
										{filteredSpecializations.map((spec) => {
											const specCourses = spec.courseIds
												.map((id) => activeCourses.find((c) => c.id === id))
												.filter((c): c is NonNullable<typeof c> => Boolean(c));
											const specDownloads = specCourses
												.map((c) =>
													data.courseDownloads.find(
														(cd) => cd.courseId === c.id,
													),
												)
												.filter((cd): cd is NonNullable<typeof cd> =>
													Boolean(cd),
												);
											const totalItems = specDownloads.reduce(
												(acc, cd) => acc + cd.totalItems,
												0,
											);
											const downloadedItems = specDownloads.reduce(
												(acc, cd) => acc + cd.downloadedItems,
												0,
											);
											const firstCourse = specCourses[0];

											return (
												<Paper
													key={String(spec.id)}
													withBorder
													className="library-spec-card"
												>
													<div className="library-spec-header">
														<Group
															gap="sm"
															wrap="nowrap"
															style={{ minWidth: 0, flex: 1 }}
														>
															<span className="course-tile">
																<SquaresFourIcon size={18} weight="duotone" />
															</span>
															<div style={{ minWidth: 0, flex: 1 }}>
																<Title order={3} lineClamp={1}>
																	{spec.name.trim()}
																</Title>
																<Text size="xs" c="dimmed" mt={2}>
																	{specCourses.length} course
																	{specCourses.length === 1 ? "" : "s"}
																	{totalItems > 0 &&
																		` · ${downloadedItems}/${totalItems} items`}
																</Text>
															</div>
														</Group>
														{firstCourse && (
															<Button
																component={Link}
																to="/courses/$courseSlug"
																params={{ courseSlug: firstCourse.slug }}
																size="compact-xs"
																variant="light"
																leftSection={
																	<PlayIcon size={12} weight="fill" />
																}
															>
																Open
															</Button>
														)}
													</div>

													<div className="library-spec-courses">
														{specCourses.map((course, idx) => {
															const cd = data.courseDownloads.find(
																(item) => item.courseId === course.id,
															);
															return (
																<Link
																	key={String(course.id)}
																	to="/courses/$courseSlug"
																	params={{ courseSlug: course.slug }}
																	className="library-spec-course-row"
																>
																	<span className="small-order">{idx + 1}</span>
																	<div
																		style={{
																			minWidth: 0,
																			flex: 1,
																		}}
																	>
																		<Text size="xs" fw={500} lineClamp={1}>
																			{course.name.trim()}
																		</Text>
																	</div>
																	<Group gap="xs" wrap="nowrap">
																		{cd && cd.totalItems > 0 && (
																			<Text size="xs" c="dimmed">
																				{cd.downloadedItems}/{cd.totalItems}
																			</Text>
																		)}
																		{cd && <StatusBadge state={cd.state} />}
																		<CaretRightIcon
																			size={13}
																			className="row-caret"
																		/>
																	</Group>
																</Link>
															);
														})}
													</div>
												</Paper>
											);
										})}
									</SimpleGrid>
								</section>
							)}

						{filter !== "specializations" && (
							<section>
								<Group justify="space-between" align="center" mb="sm">
									<Group gap="xs" align="center">
										<Title order={2} className="section-title">
											Courses
										</Title>
										<Badge size="xs" variant="light" color="gray">
											{filteredCourses.length}
										</Badge>
									</Group>
								</Group>

								{filteredCourses.length === 0 ? (
									<Paper withBorder className="panel">
										<EmptyState
											icon={BooksIcon}
											title={
												activeCourses.length
													? "No matching courses"
													: "No courses in library"
											}
											action={
												activeCourses.length ? (
													<Button
														variant="light"
														size="xs"
														onClick={() => {
															setQuery("");
															setFilter("all");
														}}
													>
														Reset filters
													</Button>
												) : (
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
												)
											}
										/>
									</Paper>
								) : (
									<SimpleGrid cols={{ base: 1, sm: 2, lg: 3 }} spacing="md">
										{filteredCourses.map((course) => {
											const cd = data.courseDownloads.find(
												(item) => item.courseId === course.id,
											);
											const progress =
												cd && cd.totalItems > 0
													? Math.round(
															(cd.downloadedItems / cd.totalItems) * 100,
														)
													: 0;
											const specName = course.specializationIds
												.map(
													(id) =>
														data.specializations.find((s) => s.id === id)?.name,
												)
												.find(Boolean);

											return (
												<Paper
													key={String(course.id)}
													withBorder
													component={Link}
													to="/courses/$courseSlug"
													params={{ courseSlug: course.slug }}
													className="library-course-card"
												>
													<div className="library-course-body">
														<Group gap="sm" align="flex-start" wrap="nowrap">
															{course.imageUrl ? (
																<img
																	src={course.imageUrl}
																	alt=""
																	className="library-course-thumb"
																	onError={(e) => {
																		e.currentTarget.style.display = "none";
																	}}
																/>
															) : (
																<span className="course-tile">
																	<BooksIcon size={18} weight="duotone" />
																</span>
															)}
															<div style={{ minWidth: 0, flex: 1 }}>
																<Title order={3} lineClamp={2}>
																	{course.name.trim()}
																</Title>
																{specName && (
																	<Text
																		size="xs"
																		c="dimmed"
																		lineClamp={1}
																		mt={3}
																	>
																		{specName.trim()}
																	</Text>
																)}
															</div>
														</Group>

														<div className="library-course-meta">
															<Group justify="space-between" gap="xs" mb={6}>
																<Text size="xs" c="dimmed">
																	{cd?.moduleCount ?? course.moduleCount}{" "}
																	modules
																	{cd ? ` · ${cd.lessonCount} lessons` : ""}
																	{cd &&
																		cd.downloadedBytes > 0n &&
																		` · ${bytes(cd.downloadedBytes)}`}
																</Text>
																{cd && cd.totalItems > 0 && (
																	<Text size="xs" fw={500}>
																		{cd.downloadedItems}/{cd.totalItems}
																	</Text>
																)}
															</Group>
															<Progress
																value={progress}
																size={4}
																color={progress === 100 ? "teal" : "myColor"}
															/>
														</div>
													</div>

													<div className="library-course-footer">
														<StatusBadge
															state={cd ? cd.state : course.outlineState}
														/>
														<Group gap={4} c="dimmed">
															<Text size="xs" fw={500} c="gray.3">
																Open course
															</Text>
															<CaretRightIcon size={13} />
														</Group>
													</div>
												</Paper>
											);
										})}
									</SimpleGrid>
								)}
							</section>
						)}
					</Stack>
				) : null}
			</main>
		</div>
	);
}
