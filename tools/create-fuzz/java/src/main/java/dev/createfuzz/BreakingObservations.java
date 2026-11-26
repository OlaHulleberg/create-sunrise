package dev.createfuzz;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.simibubi.create.content.kinetics.base.BlockBreakingKineticBlockEntity;
import net.minecraft.core.BlockPos;
import org.jetbrains.annotations.Nullable;

import java.util.List;

/** A breaker's live state (`BlockBreakingKineticBlockEntity`), read from its fields. The game
 * keeps no crack overlay on the server (1.21.1 sends it to clients): what a breaker last asked
 * the level to show is its `destroyProgress` at its `breakingPos`. */
final class BreakingObservations {
	/** `Progress` and `NextTick` as `write` saves them. */
	static JsonObject breaker(BlockBreakingKineticBlockEntity be) {
		JsonObject o = new JsonObject();
		o.addProperty("progress", progress(be));
		o.addProperty("next_tick", (int) TransportObservations.<Integer>field(be, BlockBreakingKineticBlockEntity.class, "ticksUntilNextProgress"));
		return o;
	}

	static int progress(BlockBreakingKineticBlockEntity be) {
		return TransportObservations.field(be, BlockBreakingKineticBlockEntity.class, "destroyProgress");
	}

	/** Where the breaker shows cracks, null when it shows none. */
	static @Nullable BlockPos cracksAt(BlockBreakingKineticBlockEntity be) {
		BlockPos breaking = TransportObservations.field(be, BlockBreakingKineticBlockEntity.class, "breakingPos");
		return progress(be) == 0 || breaking == null ? null : breaking;
	}

	static JsonArray cracks(List<Integer> stages) {
		JsonArray a = new JsonArray();
		stages.forEach(a::add);
		return a;
	}
}
