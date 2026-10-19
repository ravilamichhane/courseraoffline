import {
	ActionIcon,
	Button,
	Group,
	Menu,
	Paper,
	SimpleGrid,
	Text,
	TextInput,
	Title,
} from "@mantine/core";
import {
	ArrowClockwiseIcon,
	ArrowRightIcon,
	DotsThreeIcon,
	DownloadSimpleIcon,
	EyeIcon,
	MagnifyingGlassIcon,
	PlusIcon,
	SquaresFourIcon,
	TrashIcon,
} from "@phosphor-icons/react";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { adminClient } from "../admin/api";
import { useAdmin } from "../admin/context";
import { CatalogDetails, DeleteDialog, ImportDialog } from "../admin/editors";
import { EmptyState, PageHeading } from "../admin/shared";
import type { Specialization } from "../gen/proto/admin_pb";

export const Route = createFileRoute("/admin/specializations")({
	validateSearch: (
		search: Record<string, unknown>,
	): { add?: boolean; edit?: string } => ({
		add: search.add === true || search.add === "true" || undefined,
		edit:
			typeof search.edit === "string" && /^\d+$/.test(search.edit)
				? search.edit
				: undefined,
	}),
	component: Specializations,
});

function Specializations() {
	const { data, run } = useAdmin();
	const search = Route.useSearch();
	const navigate = Route.useNavigate();
	const [query, setQuery] = useState("");
	const [deleting, setDeleting] = useState<Specialization | null>(null);
	if (!data) return null;
	const filtered = data.specializations.filter((s) =>
		`${s.name} ${s.slug}`.toLowerCase().includes(query.toLowerCase()),
	);
	const editing = data.specializations.find(
		(s) => String(s.id) === search.edit,
	);
	const close = () => {
		void navigate({ search: {}, replace: true });
	};
	const add = () => {
		void navigate({ search: { add: true } });
	};
	return (
		<>
			<PageHeading
				title="Specialisations"
				action={
					<Button onClick={add} size="xs" leftSection={<PlusIcon size={15} />}>
						Import specialisation
					</Button>
				}
			/>
			<Group justify="space-between" mb="md">
				<TextInput
					aria-label="Search specialisations"
					placeholder="Search specialisations…"
					leftSection={<MagnifyingGlassIcon size={16} />}
					value={query}
					onChange={(e) => setQuery(e.currentTarget.value)}
					size="xs"
					w={280}
					maw="100%"
				/>
				<Text c="dimmed" size="xs">
					{filtered.length} specialisation{filtered.length === 1 ? "" : "s"}
				</Text>
			</Group>
			{!filtered.length ? (
				<Paper withBorder className="panel">
					<EmptyState
						icon={SquaresFourIcon}
						title={
							data.specializations.length
								? "No specialisations found"
								: "No specialisations yet"
						}
						action={
							data.specializations.length ? (
								<Button variant="light" size="xs" onClick={() => setQuery("")}>
									Clear search
								</Button>
							) : (
								<Button
									variant="light"
									size="xs"
									onClick={add}
									leftSection={<PlusIcon size={14} />}
								>
									Import specialisation
								</Button>
							)
						}
					/>
				</Paper>
			) : (
				<SimpleGrid cols={{ base: 1, md: 2, xl: 3 }} spacing="md">
					{filtered.map((s) => (
						<Paper
							key={String(s.id)}
							withBorder
							className="specialization-card"
						>
							<div className="specialization-body">
								<Group
									justify="space-between"
									wrap="nowrap"
									align="center"
									gap="xs"
								>
									<Title order={3} lineClamp={1}>
										{s.name}
									</Title>
									<Menu position="bottom-end" width={180}>
										<Menu.Target>
											<ActionIcon
												variant="subtle"
												color="gray"
												aria-label={`Actions for ${s.name}`}
											>
												<DotsThreeIcon size={20} weight="bold" />
											</ActionIcon>
										</Menu.Target>
										<Menu.Dropdown>
											<Menu.Item
												leftSection={<DownloadSimpleIcon size={15} />}
												disabled={!s.courseIds.length}
												onClick={() =>
													void run(
														() =>
															adminClient.startDownload({
																target: {
																	case: "specializationId",
																	value: s.id,
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
															adminClient.importSpecialization(
																{ slug: s.slug },
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
													void navigate({ search: { edit: String(s.id) } })
												}
											>
												Details
											</Menu.Item>
											<Menu.Divider />
											<Menu.Item
												color="red"
												leftSection={<TrashIcon size={15} />}
												onClick={() => setDeleting(s)}
											>
												Delete
											</Menu.Item>
										</Menu.Dropdown>
									</Menu>
								</Group>
								<div className="specialization-course-list">
									{s.courseIds.slice(0, 3).map((id, i) => (
										<Group key={String(id)} gap="xs" wrap="nowrap">
											<span className="small-order">{i + 1}</span>
											<Text size="xs" lineClamp={1}>
												{data.courses.find((c) => c.id === id)?.name ||
													"Unavailable course"}
											</Text>
										</Group>
									))}
									{s.courseIds.length === 0 && (
										<Text c="dimmed" size="xs">
											No courses
										</Text>
									)}
									{s.courseIds.length > 3 && (
										<Text c="dimmed" size="xs">
											+{s.courseIds.length - 3} more
										</Text>
									)}
								</div>
							</div>
							<Group justify="space-between" className="specialization-footer">
								<Text size="xs" c="dimmed">
									{s.courseIds.length} course
									{s.courseIds.length === 1 ? "" : "s"}
								</Text>
								<Button
									size="compact-xs"
									variant="subtle"
									color="gray"
									rightSection={<ArrowRightIcon size={13} />}
									onClick={() =>
										void navigate({ search: { edit: String(s.id) } })
									}
								>
									Details
								</Button>
							</Group>
						</Paper>
					))}
				</SimpleGrid>
			)}
			{search.add && <ImportDialog kind="specialisation" close={close} />}
			{editing && <CatalogDetails specialization={editing} close={close} />}
			{deleting && (
				<DeleteDialog
					kind="specialisation"
					name={deleting.name}
					close={() => setDeleting(null)}
					onDelete={() =>
						run(
							() => adminClient.deleteSpecialization({ id: deleting.id }),
							"Specialisation deleted.",
						)
					}
				/>
			)}
		</>
	);
}
