import {
	ActionIcon,
	Badge,
	Button,
	Group,
	Menu,
	Pagination,
	Paper,
	Select,
	Table,
	Text,
	TextInput,
} from "@mantine/core";
import {
	ArchiveIcon,
	ArrowClockwiseIcon,
	BooksIcon,
	DotsThreeIcon,
	DownloadSimpleIcon,
	EyeIcon,
	MagnifyingGlassIcon,
	PlusIcon,
	TrashIcon,
} from "@phosphor-icons/react";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { adminClient, date } from "../admin/api";
import { useAdmin } from "../admin/context";
import { CatalogDetails, DeleteDialog, ImportDialog } from "../admin/editors";
import { EmptyState, PageHeading, StatusBadge } from "../admin/shared";
import type { Course } from "../gen/proto/admin_pb";

export const Route = createFileRoute("/admin/courses")({
	validateSearch: (
		search: Record<string, unknown>,
	): { add?: boolean; edit?: string } => ({
		add: search.add === true || search.add === "true" || undefined,
		edit:
			typeof search.edit === "string" && /^\d+$/.test(search.edit)
				? search.edit
				: undefined,
	}),
	component: Courses,
});

function Courses() {
	const { data, run } = useAdmin();
	const search = Route.useSearch();
	const navigate = Route.useNavigate();
	const [query, setQuery] = useState("");
	const [status, setStatus] = useState<string | null>("all");
	const [specialization, setSpecialization] = useState<string | null>("all");
	const [page, setPage] = useState(1);
	const [deleting, setDeleting] = useState<Course | null>(null);
	if (!data) return null;
	const filtered = data.courses.filter(
		(c) =>
			`${c.name} ${c.slug} ${c.remoteId}`
				.toLowerCase()
				.includes(query.toLowerCase()) &&
			(status === "all" ||
				(status === "archived"
					? c.archived
					: !c.archived &&
						(status === "active" || c.metadataState === status))) &&
			(specialization === "all" ||
				c.specializationIds.some((id) => String(id) === specialization)),
	);
	const pages = Math.max(1, Math.ceil(filtered.length / 10));
	const currentPage = Math.min(page, pages);
	const editing = data.courses.find((c) => String(c.id) === search.edit);
	const close = () => {
		void navigate({ search: {}, replace: true });
	};
	const add = () => {
		void navigate({ search: { add: true } });
	};
	return (
		<>
			<PageHeading
				title="Courses"
				action={
					<Button onClick={add} size="xs" leftSection={<PlusIcon size={15} />}>
						Import course
					</Button>
				}
			/>
			<Paper withBorder className="panel catalog-panel">
				<div className="catalog-toolbar">
					<TextInput
						aria-label="Search courses"
						placeholder="Search courses…"
						leftSection={<MagnifyingGlassIcon size={16} />}
						value={query}
						onChange={(e) => {
							setQuery(e.currentTarget.value);
							setPage(1);
						}}
						size="xs"
						className="catalog-search"
					/>
					<Group gap="xs" wrap="wrap">
						<Select
							aria-label="Filter courses by status"
							value={status}
							onChange={(v) => {
								setStatus(v);
								setPage(1);
							}}
							data={[
								{ value: "all", label: "All statuses" },
								{ value: "active", label: "Active" },
								{ value: "ready", label: "Ready" },
								{ value: "summary", label: "Imported" },
								{ value: "failed", label: "Failed" },
								{ value: "archived", label: "Archived" },
							]}
							size="xs"
							w={140}
							allowDeselect={false}
						/>
						<Select
							aria-label="Filter by specialisation"
							value={specialization}
							onChange={(v) => {
								setSpecialization(v);
								setPage(1);
							}}
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
					</Group>
				</div>
				{!filtered.length ? (
					<EmptyState
						title={data.courses.length ? "No courses found" : "No courses yet"}
						action={
							data.courses.length ? (
								<Button
									variant="light"
									size="xs"
									onClick={() => {
										setQuery("");
										setStatus("all");
										setSpecialization("all");
									}}
								>
									Clear filters
								</Button>
							) : (
								<Button
									variant="light"
									size="xs"
									onClick={add}
									leftSection={<PlusIcon size={14} />}
								>
									Import course
								</Button>
							)
						}
					/>
				) : (
					<>
						<Table.ScrollContainer minWidth={720}>
							<Table
								verticalSpacing="sm"
								horizontalSpacing="md"
								className="catalog-table"
							>
								<Table.Thead>
									<Table.Tr>
										<Table.Th>Course</Table.Th>
										<Table.Th>Specialisations</Table.Th>
										<Table.Th>Content</Table.Th>
										<Table.Th>Status</Table.Th>
										<Table.Th>Updated</Table.Th>
										<Table.Th w={48} ta="right">
											<span className="sr-only">Actions</span>
										</Table.Th>
									</Table.Tr>
								</Table.Thead>
								<Table.Tbody>
									{filtered
										.slice((currentPage - 1) * 10, currentPage * 10)
										.map((c) => {
											const cd = data.courseDownloads.find(
												(item) => item.courseId === c.id,
											);
											return (
												<Table.Tr key={String(c.id)}>
													<Table.Td>
														<Group wrap="nowrap" gap="sm">
															<span className="course-tile">
																<BooksIcon size={18} weight="duotone" />
															</span>
															<div style={{ minWidth: 0 }}>
																<button
																	type="button"
																	className="course-title-button"
																	onClick={() =>
																		void navigate({
																			search: { edit: String(c.id) },
																		})
																	}
																>
																	{c.name}
																</button>
																<Text size="xs" c="dimmed" mt={1} lineClamp={1}>
																	{c.slug}
																</Text>
															</div>
														</Group>
													</Table.Td>
													<Table.Td>
														{c.specializationIds.length ? (
															<Badge variant="light" color="gray" size="sm">
																{c.specializationIds.length} specialisation
																{c.specializationIds.length === 1 ? "" : "s"}
															</Badge>
														) : (
															<Text size="xs" c="dimmed">
																Standalone
															</Text>
														)}
													</Table.Td>
													<Table.Td>
														<Text size="xs" fw={500}>
															{c.moduleCount} modules
															{cd ? ` · ${cd.lessonCount} lessons` : ""}
														</Text>
														{cd && cd.totalItems > 0 && (
															<Text size="xs" c="dimmed" mt={1}>
																{cd.downloadedItems}/{cd.totalItems} downloaded
															</Text>
														)}
													</Table.Td>
													<Table.Td>
														<StatusBadge
															state={c.archived ? "archived" : c.metadataState}
														/>
													</Table.Td>
													<Table.Td>
														<Text c="dimmed" size="xs">
															{date(c.updatedAt)}
														</Text>
													</Table.Td>
													<Table.Td ta="right">
														<Menu position="bottom-end" shadow="md" width={180}>
															<Menu.Target>
																<ActionIcon
																	variant="subtle"
																	color="gray"
																	aria-label={`Actions for ${c.name}`}
																>
																	<DotsThreeIcon size={20} weight="bold" />
																</ActionIcon>
															</Menu.Target>
															<Menu.Dropdown>
																<Menu.Item
																	leftSection={<DownloadSimpleIcon size={15} />}
																	disabled={c.archived}
																	onClick={() =>
																		void run(
																			() =>
																				adminClient.startDownload({
																					target: {
																						case: "courseId",
																						value: c.id,
																					},
																				}),
																			"Download queued.",
																		)
																	}
																>
																	Download
																</Menu.Item>
																<Menu.Item
																	leftSection={<ArrowClockwiseIcon size={15} />}
																	onClick={() =>
																		void run(
																			() =>
																				adminClient.importCourse(
																					{ slug: c.slug },
																					{ timeoutMs: 180000 },
																				),
																			"Refreshed from Coursera.",
																		)
																	}
																>
																	Refresh
																</Menu.Item>
																<Menu.Item
																	leftSection={<EyeIcon size={15} />}
																	onClick={() =>
																		void navigate({
																			search: { edit: String(c.id) },
																		})
																	}
																>
																	Details
																</Menu.Item>
																<Menu.Item
																	leftSection={<ArchiveIcon size={15} />}
																	onClick={() =>
																		void run(
																			() =>
																				adminClient.setCourseArchived({
																					id: c.id,
																					archived: !c.archived,
																				}),
																			c.archived
																				? "Course restored."
																				: "Course archived.",
																		)
																	}
																>
																	{c.archived ? "Restore" : "Archive"}
																</Menu.Item>
																<Menu.Divider />
																<Menu.Item
																	color="red"
																	leftSection={<TrashIcon size={15} />}
																	onClick={() => setDeleting(c)}
																>
																	Delete
																</Menu.Item>
															</Menu.Dropdown>
														</Menu>
													</Table.Td>
												</Table.Tr>
											);
										})}
								</Table.Tbody>
							</Table>
						</Table.ScrollContainer>
						<Group justify="space-between" className="table-footer">
							<Text c="dimmed" size="xs">
								{(currentPage - 1) * 10 + 1}–
								{Math.min(currentPage * 10, filtered.length)} of{" "}
								{filtered.length}
							</Text>
							{pages > 1 && (
								<Pagination
									size="xs"
									value={currentPage}
									onChange={setPage}
									total={pages}
								/>
							)}
						</Group>
					</>
				)}
			</Paper>
			{search.add && <ImportDialog kind="course" close={close} />}
			{editing && <CatalogDetails course={editing} close={close} />}
			{deleting && (
				<DeleteDialog
					kind="course"
					name={deleting.name}
					close={() => setDeleting(null)}
					onDelete={() =>
						run(
							() => adminClient.deleteCourse({ id: deleting.id }),
							"Course deleted.",
						)
					}
				/>
			)}
		</>
	);
}
