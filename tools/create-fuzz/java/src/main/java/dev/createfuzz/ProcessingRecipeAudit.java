package dev.createfuzz;

import com.google.gson.*;
import com.simibubi.create.AllRecipeTypes;
import com.simibubi.create.content.processing.recipe.ProcessingRecipe;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeType;
import java.util.*;

/** Checks the driver's processing recipes (`FUZZ_PROCESSING_MANIFEST`) against the loaded data: each
 * entry as Create reads it, and for each input the recipes of each machine's types that take it. */
final class ProcessingRecipeAudit {
 static void check(ServerLevel level) {
  JsonObject doc;
  try {doc=JsonParser.parseString(java.nio.file.Files.readString(java.nio.file.Path.of(System.getenv("FUZZ_PROCESSING_MANIFEST")))).getAsJsonObject();}
  catch(java.io.IOException e) {throw new IllegalStateException(e);}
  var expected=new TreeMap<String,TreeSet<String>>();
  for(var e:doc.getAsJsonArray("recipes")) {
   var r=e.getAsJsonObject();String id=r.get("id").getAsString();
   var holder=level.getRecipeManager().byKey(ResourceLocation.parse(id)).orElseThrow(()->new IllegalStateException("processing recipe missing: "+id));
   if(!(holder.value() instanceof ProcessingRecipe<?,?> recipe)) throw new IllegalStateException("not a processing recipe: "+id);
   var input=new ItemStack(BuiltInRegistries.ITEM.get(ResourceLocation.parse(r.get("input").getAsString())));
   var results=r.getAsJsonArray("results");var outputs=recipe.getRollableResults();
   boolean same=recipe.getIngredients().size()==1&&recipe.getIngredients().getFirst().test(input)
    &&recipe.getProcessingDuration()==r.get("time").getAsInt()&&outputs.size()==results.size();
   for(int i=0;same&&i<outputs.size();i++) {
    var want=results.get(i).getAsJsonObject();var out=outputs.get(i);
    same=BuiltInRegistries.ITEM.getKey(out.getStack().getItem()).toString().equals(want.get("item").getAsString())
     &&out.getStack().getCount()==want.get("count").getAsInt()&&out.getChance()==want.get("chance").getAsFloat();
   }
   if(!same) throw new IllegalStateException("processing recipe contract: "+id);
   expected.computeIfAbsent(r.get("type").getAsString()+" "+r.get("input").getAsString(),k->new TreeSet<>()).add(id);
  }
  var types=Map.of("milling",AllRecipeTypes.MILLING.getType(),"crushing",AllRecipeTypes.CRUSHING.getType(),
   "cutting",AllRecipeTypes.CUTTING.getType(),"stonecutting",RecipeType.STONECUTTING);
  var machines=Map.of("millstone",List.of("milling"),"crusher",List.of("crushing","milling"),"saw",List.of("cutting","stonecutting"));
  for(var m:doc.getAsJsonObject("inputs").entrySet()) for(var e:m.getValue().getAsJsonArray()) {
   String item=e.getAsString();var stack=new ItemStack(BuiltInRegistries.ITEM.get(ResourceLocation.parse(item)));
   for(var t:machines.get(m.getKey())) {
    var found=new TreeSet<String>();
    for(var h:level.getRecipeManager().getRecipes()) {
     if(h.value().getType()!=types.get(t)||h.value().getIngredients().isEmpty()) continue;
     if(h.value().getIngredients().getFirst().test(stack)) found.add(h.id().toString());
    }
    var want=expected.getOrDefault(t+" "+item,new TreeSet<>());
    if(!found.equals(want)) throw new IllegalStateException("processing recipes of "+t+" for "+item+": "+found+" not "+want);
   }
  }
 }
}
