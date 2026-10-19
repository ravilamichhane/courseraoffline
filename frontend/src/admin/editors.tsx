import {
	Alert,
	Badge,
	Button,
	Divider,
	Group,
	Modal,
	Stack,
	Text,
	TextInput,
} from "@mantine/core";
import {
	ArrowClockwiseIcon,
	DownloadSimpleIcon,
	PlusIcon,
	TreeStructureIcon,
} from "@phosphor-icons/react";
import { type FormEvent, useState } from "react";
import type { Course, Specialization } from "../gen/proto/admin_pb";
import { adminClient, errorMessage } from "./api";
import { useAdmin } from "./context";
import { DownloadStructureModal } from "./downloads";
import { StatusBadge } from "./shared";

export function ImportDialog({
	kind,
	close,
}: {
	kind: "course" | "specialisation";
	close: () => void;
}) {
	const { refresh } = useAdmin();
	const [slug, setSlug] = useState("");
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		setSaving(true);
		setError(null);
		try {
			if (kind === "course")
				await adminClient.importCourse(
					{ slug: slug.trim() },
					{ timeoutMs: 180000 },
				);
			else
				await adminClient.importSpecialization(
					{ slug: slug.trim() },
					{ timeoutMs: 180000 },
				);
			await refresh();
			close();
		} catch (error) {
			setError(errorMessage(error));
			setSaving(false);
		}
	};
	return (
		<Modal
			opened
			title={`Import ${kind}`}
			centered
			onClose={() => {
				if (!saving) close();
			}}
			closeOnClickOutside={!saving}
			closeOnEscape={!saving}
			size="sm"
		>
			<form onSubmit={submit}>
				<Stack gap="md">
					<TextInput
						label="Coursera slug"
						placeholder={
							kind === "course"
								? "supervised-machine-learning"
								: "machine-learning"
						}
						required
						pattern="[a-z0-9-]+"
						maxLength={200}
						autoFocus
						value={slug}
						disabled={saving}
						onChange={(e) => setSlug(e.currentTarget.value)}
					/>
					{error && <Alert color="red">{error}</Alert>}
					<Group justify="flex-end" gap="xs">
						<Button variant="default" disabled={saving} onClick={close}>
							Cancel
						</Button>
						<Button
							type="submit"
							loading={saving}
							leftSection={<PlusIcon size={15} />}
						>
							{saving ? "Importing…" : "Import"}
						</Button>
					</Group>
				</Stack>
			</form>
		</Modal>
	);
}

export function CatalogDetails({
	course,
	specialization,
	close,
}: {
	course?: Course;
	specialization?: Specialization;
	close: () => void;
}) {
	const { data, run } = useAdmin();
	const [busy, setBusy] = useState<"sync" | "download" | null>(null);
	const [showStructure, setShowStructure] = useState(false);
	const record = course || specialization;
	if (!record) return null;
	const structureCourses =
		data?.courseDownloads.filter((cd) =>
			course
				? cd.courseId === course.id
				: specialization?.courseIds.includes(cd.courseId),
		) || [];
	const totalDownloaded = structureCourses.reduce(
		(acc, c) => acc + c.downloadedItems,
		0,
	);
	const totalRemaining = structureCourses.reduce(
		(acc, c) => acc + c.remainingItems,
		0,
	);
	const start = async () => {
		setBusy("download");
		const saved = await run(
			() =>
				adminClient.startDownload({
					target: course
						? { case: "courseId", value: course.id }
						: { case: "specializationId", value: record.id },
				}),
			"Download queued.",
		);
		setBusy(null);
		if (saved) close();
	};
	return (
		<>
			<Modal
				opened={!showStructure}
				title={record.name}
				centered
				size="md"
				onClose={() => {
					if (!busy) close();
				}}
				closeOnEscape={!busy}
				closeOnClickOutside={!busy}
			>
				<Stack gap="md">
					<Group justify="space-between" align="center" gap="xs">
						<Text c="dimmed" size="xs">
							{record.slug}
						</Text>
						{course && (
							<StatusBadge
								state={course.archived ? "archived" : course.metadataState}
							/>
						)}
					</Group>
					{course && (
						<Group gap="xs">
							<Badge size="sm" color="gray" variant="light">
								{course.moduleCount} modules
							</Badge>
							<Badge size="sm" color="teal" variant="light">
								{totalDownloaded} downloaded
							</Badge>
							<Badge size="sm" color="orange" variant="light">
								{totalRemaining} remaining
							</Badge>
						</Group>
					)}
					{specialization && (
						<Stack gap={8}>
							{specialization.courseIds.map((id, i) => {
								const c = data?.courses.find((item) => item.id === id);
								const cd = data?.courseDownloads.find(
									(item) => item.courseId === id,
								);
								return (
									<div key={String(id)} className="ordered-course">
										<Group wrap="nowrap" gap="sm" justify="space-between">
											<Group wrap="nowrap" gap="sm" style={{ minWidth: 0 }}>
												<span className="order-number">{i + 1}</span>
												<Text size="xs" fw={500} lineClamp={1}>
													{c?.name || "Unavailable course"}
												</Text>
											</Group>
											{cd && (
												<Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
													{cd.downloadedItems}/{cd.totalItems}
												</Text>
											)}
										</Group>
									</div>
								);
							})}
						</Stack>
					)}
					<Divider />
					<Group justify="space-between" gap="xs">
						<Group gap="xs">
							<Button
								size="xs"
								variant="default"
								leftSection={<ArrowClockwiseIcon size={14} />}
								loading={busy === "sync"}
								disabled={busy === "download"}
								onClick={async () => {
									setBusy("sync");
									await run(
										() =>
											course
												? adminClient.importCourse(
														{ slug: record.slug },
														{ timeoutMs: 180000 },
													)
												: adminClient.importSpecialization(
														{ slug: record.slug },
														{ timeoutMs: 180000 },
													),
										"Refreshed from Coursera.",
									);
									setBusy(null);
								}}
							>
								Refresh
							</Button>
							{structureCourses.length > 0 && (
								<Button
									size="xs"
									variant="default"
									leftSection={<TreeStructureIcon size={14} />}
									onClick={() => setShowStructure(true)}
								>
									Structure
								</Button>
							)}
						</Group>
						<Button
							size="xs"
							leftSection={<DownloadSimpleIcon size={14} />}
							loading={busy === "download"}
							disabled={
								busy === "sync" ||
								course?.archived ||
								(specialization && !specialization.courseIds.length)
							}
							onClick={() => void start()}
						>
							Download
						</Button>
					</Group>
				</Stack>
			</Modal>
			{showStructure && (
				<DownloadStructureModal
					title={record.name}
					courses={structureCourses}
					close={() => setShowStructure(false)}
				/>
			)}
		</>
	);
}

export function DeleteDialog({
	name,
	kind,
	onDelete,
	close,
}: {
	name: string;
	kind: "course" | "specialisation";
	onDelete: () => Promise<boolean>;
	close: () => void;
}) {
	const [busy, setBusy] = useState(false);
	return (
		<Modal
			opened
			title={`Delete ${kind}?`}
			onClose={() => {
				if (!busy) close();
			}}
			centered
			size="sm"
			closeOnEscape={!busy}
			closeOnClickOutside={!busy}
		>
			<Text size="sm">Remove “{name}” from the library?</Text>
			<Group justify="flex-end" gap="xs" mt="lg">
				<Button variant="default" disabled={busy} onClick={close}>
					Cancel
				</Button>
				<Button
					color="red"
					loading={busy}
					onClick={async () => {
						setBusy(true);
						const deleted = await onDelete();
						setBusy(false);
						if (deleted) close();
					}}
				>
					Delete
				</Button>
			</Group>
		</Modal>
	);
}
