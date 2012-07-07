// TODO:
// create the EventSource and see what happens at the server
// close the EventSource and see what happens at the server
// probably don't need to attach the source to the window?

window.onload = function()
{
	console.log("loaded page");
	
	var timeElement = document.getElementById('uptime');
	timeElement.innerHTML = "whatever";
	
	window.source = new EventSource('/uptime');
	window.source.addEventListener('message', function(event)
	{
		timeElement.innerHTML = event.data;
	});
	
	window.source.addEventListener('open', function(event)
	{
		console.log('> Connection was opened');
	});
	
	window.source.addEventListener('error', function(event)
	{
		if (event.eventPhase == 2)
		{
			console.log('> Connection was closed');
		}
	});
}
